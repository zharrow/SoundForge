/**
 * L'onglet Musique : le morceau ouvert et son historique, la piste choisie, le rendu (seules les pistes
 * modifiées sont refaites côté natif) et la lecture synchronisée des stems.
 *
 * Lecture : un stem = une source WebAudio en boucle, toutes lancées au même instant et de même longueur, donc
 * calées à l'échantillon près. Couper un stem baisse son gain (sans arrêter les autres) : c'est ce que fera le
 * jeu en passant de l'exploration au combat.
 */

import type { Author } from "./doc";
import { SoundDoc } from "./doc";
import { clone } from "./model";
import { type MusicTake, type Note, type Voice, STEMS, newMusic, stemOf } from "./music-model";
import * as native from "./native";

export type MusicChange = "doc" | "value" | "render" | "library" | "play" | "history" | "selection";

const RENDER_DELAY = 140; // ms

export class StemPlayer {
  private context: AudioContext | null = null;
  private sources: AudioBufferSourceNode[] = [];
  private gains = new Map<string, GainNode>();
  private startedAt = 0;
  private offset = 0;
  private preview: AudioBufferSourceNode | null = null;
  buffers = new Map<string, AudioBuffer>();
  on = new Map<string, boolean>();
  playing = false;
  onState: () => void = () => {};

  get ctx(): AudioContext {
    this.context ??= new AudioContext({ sampleRate: 44100 });
    return this.context;
  }

  get duration(): number {
    return [...this.buffers.values()][0]?.duration ?? 0;
  }

  /** Position de lecture en secondes (null à l'arrêt). */
  position(): number | null {
    if (!this.playing || !this.duration) return null;
    return (this.offset + this.ctx.currentTime - this.startedAt) % this.duration;
  }

  async setBuffers(stems: Map<string, ArrayBuffer>): Promise<void> {
    const decoded = new Map<string, AudioBuffer>();
    for (const [name, bytes] of stems) decoded.set(name, await this.ctx.decodeAudioData(bytes));
    const at = this.position();
    this.buffers = decoded;
    // Pendant la lecture, le nouveau rendu prend la place de l'ancien à la même position.
    if (at !== null) this.play(at);
  }

  play(from = 0): void {
    this.stopSources();
    if (!this.buffers.size) return;
    void this.ctx.resume();
    const when = this.ctx.currentTime + 0.03;
    const offset = from % this.duration;
    for (const [name, buffer] of this.buffers) {
      let gain = this.gains.get(name);
      if (!gain) {
        gain = this.ctx.createGain();
        gain.connect(this.ctx.destination);
        this.gains.set(name, gain);
      }
      gain.gain.value = this.on.get(name) === false ? 0 : 1;
      const source = this.ctx.createBufferSource();
      source.buffer = buffer;
      source.loop = true;
      source.connect(gain);
      source.start(when, offset);
      this.sources.push(source);
    }
    this.startedAt = when;
    this.offset = offset;
    this.playing = true;
    this.onState();
  }

  stop(): void {
    this.stopSources();
    this.playing = false;
    this.onState();
  }

  toggle(): void {
    if (this.playing) this.stop();
    else this.play(this.offset);
  }

  setOn(stem: string, on: boolean): void {
    this.on.set(stem, on);
    const gain = this.gains.get(stem);
    // Un fondu de 80 ms : un stem qui entre ou sort ne claque pas.
    gain?.gain.setTargetAtTime(on ? 1 : 0, this.ctx.currentTime, 0.08 / 3);
  }

  /** Une note seule (en la posant), par-dessus la lecture. */
  async audition(bytes: ArrayBuffer): Promise<void> {
    const buffer = await this.ctx.decodeAudioData(bytes);
    try {
      this.preview?.stop();
    } catch {}
    const source = this.ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(this.ctx.destination);
    void this.ctx.resume();
    source.start();
    this.preview = source;
  }

  private stopSources(): void {
    for (const source of this.sources) {
      try {
        source.stop();
      } catch {}
    }
    this.sources = [];
  }
}

export class MusicEditor {
  doc = new SoundDoc<MusicTake>({ music: newMusic(), seed: 1 }, null, null, "Nouveau morceau");
  library: native.MusicLibrary | null = null;
  selected = 0;
  /** Note choisie dans la piste choisie (index dans `notes`), ou null. */
  note: number | null = null;
  muted = new Set<number>();
  solo = new Set<number>();
  rendered: native.RenderedMusic | null = null;
  renderError: string | null = null;
  rendering = false;
  /** Grille : durée d'un pas, en temps. */
  snap = 0.5;
  /** Zoom horizontal : pixels par temps. */
  zoom = 28;
  /** À l'ouverture, le piano roll ajuste le zoom pour montrer tout le morceau. */
  fit = true;
  showOthers = true;
  /** Les sons de la bibliothèque (onglet Sons), jouables comme instruments de synthèse. */
  soundLibrary: () => native.LibraryEntry[] = () => [];
  player = new StemPlayer();
  savedAt = 0;
  private listeners: ((change: MusicChange) => void)[] = [];
  private timer = 0;
  private sequence = 0;
  private waiters: (() => void)[] = [];

  constructor() {
    this.player.onState = () => this.emit("play");
  }

  on(listener: (change: MusicChange) => void): void {
    this.listeners.push(listener);
  }

  emit(change: MusicChange): void {
    for (const listener of this.listeners) listener(change);
  }

  get music() {
    return this.doc.take.music;
  }

  get track() {
    return this.music.tracks[this.selected];
  }

  get instruments() {
    return this.library?.instruments ?? [];
  }

  /** Les stems présents, dans l'ordre du jeu (ambiance d'abord), puis les autres. */
  get stems(): string[] {
    const present = [...new Set(this.music.tracks.map(stemOf))];
    return [...STEMS.filter((s) => present.includes(s)), ...present.filter((s) => !STEMS.includes(s))];
  }

  open(take: MusicTake, name: string | null, label: string): void {
    this.player.stop();
    this.doc = new SoundDoc<MusicTake>(clone(take), name, null, label);
    this.selected = 0;
    this.note = null;
    this.muted.clear();
    this.solo.clear();
    this.rendered = null;
    this.player.buffers.clear();
    this.fit = true;
    this.emit("doc");
    this.requestRender();
  }

  edit(label: string, change: (take: MusicTake) => void, author: Author = "user"): boolean {
    const changed = this.doc.edit(label, change, author);
    if (changed) {
      this.clampSelection();
      this.emit("doc");
      this.requestRender();
    }
    return changed;
  }

  gesture(label: string, phase: "start" | "move" | "end", change?: (take: MusicTake) => void): void {
    if (phase === "start" || !this.doc.editing) this.doc.begin(label);
    change?.(this.doc.take);
    if (phase === "end") {
      const changed = this.doc.commit();
      this.emit("history");
      if (changed) this.requestRender();
    } else {
      this.emit("value");
    }
  }

  undo(): void {
    if (this.doc.undo()) this.afterJump();
  }

  redo(): void {
    if (this.doc.redo()) this.afterJump();
  }

  gotoStep(index: number): void {
    this.doc.goto(index);
    this.afterJump();
  }

  private afterJump(): void {
    this.clampSelection();
    this.emit("doc");
    this.requestRender();
  }

  private clampSelection(): void {
    this.selected = Math.min(this.selected, Math.max(0, this.music.tracks.length - 1));
    if (this.note !== null && !this.track?.notes[this.note]) this.note = null;
  }

  select(track: number, note: number | null = null): void {
    this.selected = track;
    this.note = note;
    this.emit("selection");
  }

  setStemOn(stem: string, on: boolean): void {
    this.player.setOn(stem, on);
    this.emit("play");
  }

  toggleMute(track: number): void {
    if (!this.muted.delete(track)) this.muted.add(track);
    this.emit("doc");
    this.requestRender();
  }

  toggleSolo(track: number): void {
    if (!this.solo.delete(track)) this.solo.add(track);
    this.emit("doc");
    this.requestRender();
  }

  audible(track: number): boolean {
    return this.solo.size ? this.solo.has(track) : !this.muted.has(track);
  }

  /** Fait entendre une note de la piste choisie (en la posant ou en la déplaçant). */
  async audition(midi: number, vel = 0.7, voice: Voice = this.track.instrument): Promise<void> {
    try {
      const seconds = Math.min(1.5, (this.music.beats_per_bar * 60) / this.music.bpm / 2);
      await this.player.audition(await native.previewNote(voice, midi + (this.track?.transpose ?? 0), vel, seconds));
    } catch {
      // Une note hors tessiture ne s'entend pas : le rendu du morceau le signalera.
    }
  }

  requestRender(): void {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.renderNow(), RENDER_DELAY);
  }

  settled(): Promise<void> {
    if (!this.rendering && this.timer === 0) return Promise.resolve();
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  private async renderNow(): Promise<void> {
    this.timer = 0;
    const id = ++this.sequence;
    const music = clone(this.music);
    music.tracks = music.tracks.filter((_, i) => this.audible(i));
    this.rendering = true;
    this.emit("render");
    try {
      if (!music.tracks.length) throw new Error("Toutes les pistes sont coupées.");
      const rendered = await native.renderMusicTake(music, this.doc.take.seed);
      if (id !== this.sequence) return;
      const stems = new Map<string, ArrayBuffer>();
      for (const [name] of rendered.stems) stems.set(name, await native.musicStem(name));
      if (id !== this.sequence) return;
      for (const [name] of rendered.stems) if (!this.player.on.has(name)) this.player.on.set(name, true);
      await this.player.setBuffers(stems);
      this.rendered = rendered;
      this.renderError = null;
    } catch (error) {
      if (id !== this.sequence) return;
      this.renderError = error instanceof Error ? error.message : String(error);
    } finally {
      if (id === this.sequence) {
        this.rendering = false;
        this.emit("render");
        if (this.timer === 0) this.waiters.splice(0).forEach((resolve) => resolve());
      }
    }
  }

  async refreshLibrary(): Promise<void> {
    this.library = await native.musicLibrary();
    this.emit("library");
  }

  /** Notes de la piste choisie, triées par début, avec leur index d'origine. */
  sortedNotes(): (Note & { index: number })[] {
    return (this.track?.notes ?? []).map((n, index) => ({ ...n, index })).sort((a, b) => a.at - b.at);
  }
}
