/**
 * L'état de l'éditeur : le son ouvert, la variante écoutée, les couches coupées ou isolées, le dernier
 * rendu et la bibliothèque. Toute retouche relance un rendu (léger délai pour regrouper les
 * mouvements rapides) ; un rendu périmé, dépassé par une retouche plus récente, est ignoré.
 *
 * Les changements sont signalés par genre, pour ne redessiner que le nécessaire :
 * - "doc" : la structure a changé (ouverture, couche ajoutée, annulation) : tout se redessine ;
 * - "value" : une valeur bouge pendant un geste : les champs restent, seul l'aperçu suit ;
 * - "render", "library", "play", "history" : la partie correspondante.
 */

import { Player } from "./audio";
import { type Author, SoundDoc } from "./doc";
import { type Take, clone, newTake } from "./model";
import * as native from "./native";

export type Change = "doc" | "value" | "render" | "library" | "play" | "history";

const RENDER_DELAY = 35; // ms

export class Editor {
  doc = new SoundDoc(newTake(), null, null, "Nouveau son");
  variant = 0;
  muted = new Set<number>();
  solo = new Set<number>();
  rendered: native.Rendered | null = null;
  renderError: string | null = null;
  rendering = false;
  library: native.Library | null = null;
  /** Sons du dossier modifiés récemment par un autre outil (Claude), pour les signaler. */
  fresh = new Map<string, number>();
  player = new Player();
  autoplay = readFlag("autoplay", true);
  /** Mode simple (par défaut) : réglages en mots, cartes de couches réduites à l'essentiel. */
  simple = readFlag("simple", true);
  /** Dernier enregistrement par l'app : la surveillance du dossier ne le prend pas pour un changement extérieur. */
  savedAt = 0;
  private listeners: ((change: Change) => void)[] = [];
  private timer = 0;
  private sequence = 0;
  private playNext = false;
  private waiters: ((r: native.Rendered | null) => void)[] = [];

  constructor() {
    this.player.onState = () => this.emit("play");
  }

  on(listener: (change: Change) => void): void {
    this.listeners.push(listener);
  }

  emit(change: Change): void {
    for (const listener of this.listeners) listener(change);
  }

  // ---------- Document ----------

  open(doc: SoundDoc, play = this.autoplay): void {
    this.player.stop();
    this.doc = doc;
    this.variant = 0;
    this.muted.clear();
    this.solo.clear();
    this.rendered = null;
    this.emit("doc");
    this.requestRender(play);
  }

  /** Retouche en une fois (bouton, menu, liste) : une étape, puis tout se redessine. */
  edit(label: string, change: (take: Take) => void, author: Author = "user"): boolean {
    const changed = this.doc.edit(label, change, author);
    if (changed) {
      this.clampVariant();
      this.emit("doc");
      this.requestRender(this.autoplay);
    }
    return changed;
  }

  /** Geste continu (curseur, glisser) : début, mouvements, fin. Une seule étape d'historique. */
  gesture(label: string, phase: "start" | "move" | "end", change?: (take: Take) => void): void {
    if (phase === "start" || !this.doc.editing) this.doc.begin(label);
    change?.(this.doc.take);
    if (phase === "end") {
      const changed = this.doc.commit();
      this.clampVariant();
      this.emit("history");
      if (changed) this.requestRender(this.autoplay);
    } else {
      this.emit("value");
      this.requestRender(false);
    }
  }

  undo(): void {
    if (this.doc.undo()) this.afterJump("Annulé");
  }

  redo(): void {
    if (this.doc.redo()) this.afterJump("Rétabli");
  }

  gotoStep(index: number): void {
    this.doc.goto(index);
    this.afterJump();
  }

  private afterJump(_label?: string): void {
    this.clampVariant();
    this.emit("doc");
    this.requestRender(this.autoplay);
  }

  private clampVariant(): void {
    const count = Math.max(1, this.doc.take.variants);
    if (this.variant >= count) this.variant = count - 1;
  }

  // ---------- Écoute ----------

  setVariant(index: number, play = true): void {
    const count = Math.max(1, this.doc.take.variants);
    this.variant = ((index % count) + count) % count;
    this.emit("render");
    this.requestRender(play);
  }

  toggleMute(layer: number): void {
    if (!this.muted.delete(layer)) this.muted.add(layer);
    this.emit("doc");
    this.requestRender(this.autoplay);
  }

  toggleSolo(layer: number): void {
    if (!this.solo.delete(layer)) this.solo.add(layer);
    this.emit("doc");
    this.requestRender(this.autoplay);
  }

  /** Couches entendues : sans les couches coupées, ou seulement les isolées. */
  audible(index: number): boolean {
    return this.solo.size ? this.solo.has(index) : !this.muted.has(index);
  }

  get partial(): boolean {
    return this.doc.take.sound.layers.some((_, i) => !this.audible(i));
  }

  /** Après suppression ou déplacement d'une couche, les couches coupées ou isolées gardent leur couche. */
  remapLayers(map: (index: number) => number | null): void {
    for (const set of [this.muted, this.solo]) {
      const next = [...set].map(map).filter((i): i is number => i !== null);
      set.clear();
      next.forEach((i) => set.add(i));
    }
  }

  /** « Avant » : la version enregistrée (ou celle de l'ouverture), même variante. Rend sa durée. */
  async playBefore(): Promise<number> {
    const rendered = await native.render(this.doc.reference, Math.min(this.variant, this.doc.reference.variants - 1));
    return this.player.playBuffer(await this.player.decode(rendered.wav));
  }

  /** Avant puis après, à la suite, pour entendre ce que les retouches ont changé. */
  async compare(): Promise<void> {
    const duration = await this.playBefore();
    // « Après » ne part que si rien d'autre n'a été lancé entre-temps.
    window.setTimeout(() => (!this.player.playing || this.player.other) && this.player.play(), (duration + 0.35) * 1000);
  }

  setSimple(on: boolean): void {
    this.simple = on;
    writeFlag("simple", on);
    this.emit("doc");
  }

  setAutoplay(on: boolean): void {
    this.autoplay = on;
    writeFlag("autoplay", on);
    this.emit("play");
  }

  setLoop(on: boolean): void {
    this.player.loop = on;
    this.emit("play");
  }

  // ---------- Rendu ----------

  requestRender(play = false): void {
    this.playNext ||= play;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.renderNow(), RENDER_DELAY);
  }

  /** Attend le rendu en cours ou demandé (pont avec Claude). */
  settled(): Promise<native.Rendered | null> {
    if (!this.rendering && this.timer === 0) return Promise.resolve(this.rendered);
    return new Promise((resolve) => this.waiters.push(resolve));
  }

  private async renderNow(): Promise<void> {
    this.timer = 0;
    const id = ++this.sequence;
    const play = this.playNext;
    this.playNext = false;
    const take = this.audibleTake();
    this.rendering = true;
    try {
      if (!take.sound.layers.length) throw new Error("Aucune couche à entendre : toutes sont coupées.");
      const rendered = await native.render(take, this.variant);
      if (id !== this.sequence) return;
      await this.player.load(rendered.wav);
      if (id !== this.sequence) return;
      this.rendered = rendered;
      this.renderError = null;
      if (play) this.player.play();
    } catch (error) {
      if (id !== this.sequence) return;
      this.renderError = error instanceof Error ? error.message : String(error);
    } finally {
      if (id === this.sequence) {
        this.rendering = false;
        this.emit("render");
        if (this.timer === 0) this.waiters.splice(0).forEach((resolve) => resolve(this.rendered));
      }
    }
  }

  private audibleTake(): Take {
    const take = clone(this.doc.take);
    if (this.partial) take.sound.layers = take.sound.layers.filter((_, i) => this.audible(i));
    return take;
  }

  // ---------- Bibliothèque ----------

  async refreshLibrary(changed: string[] = []): Promise<void> {
    const now = Date.now();
    for (const name of changed) this.fresh.set(name, now);
    this.library = await native.library();
    this.emit("library");
  }
}

function readFlag(key: string, fallback: boolean): boolean {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : value === "1";
  } catch {
    return fallback;
  }
}

function writeFlag(key: string, on: boolean): void {
  try {
    localStorage.setItem(key, on ? "1" : "0");
  } catch {}
}
