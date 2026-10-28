/**
 * Le piano roll de la piste choisie : le temps de gauche à droite (mesures et temps), la hauteur de bas en haut.
 *
 * - Clic dans le vide : une note naît (et s'allonge si on tire vers la droite) ; on l'entend.
 * - Glisser une note : la déplacer (calée sur la grille) ; tirer son bord droit : la rallonger.
 * - Double-clic : la supprimer. Clavier : ↑ ↓ (⇧ : octave), ← → (un pas de grille), Suppr.
 * - Les notes jouées par un motif sont hachurées et non modifiables ici (« Détacher les motifs » les rend
 *   à la piste) ; les autres pistes apparaissent en filigrane.
 * - Clic sur la règle : la lecture part de là.
 */

import type { MusicEditor } from "../music-editor";
import { type Note, frenchName, isBlackKey, midiOf, noteName, patternNotes } from "../music-model";
import { layerColor } from "./layers";

const ROW = 12; // px par demi-ton
const KEYS = 52; // largeur du clavier
const RULER = 22; // hauteur de la règle
const EDGE = 6; // px : zone de saisie du bord droit d'une note

export class PianoRoll {
  private keys = document.createElement("canvas");
  private ruler = document.createElement("canvas");
  private grid = document.createElement("canvas");
  private scroller = document.createElement("div");
  private low = 36;
  private high = 84;
  private lastLen = 1;
  private lastVel = 0.7;
  private drag: { mode: "move" | "resize" | "create"; index: number; dx: number; startAt: number; pitch: number } | null = null;
  private scrolledTo = "";

  constructor(
    private root: HTMLElement,
    private editor: MusicEditor,
  ) {
    root.classList.add("pianoroll");
    const corner = document.createElement("div");
    corner.className = "pr-corner";
    this.keys.className = "pr-keys";
    this.ruler.className = "pr-ruler";
    this.scroller.className = "pr-scroll";
    this.grid.className = "pr-grid";
    this.scroller.append(this.grid);
    root.append(corner, this.ruler, this.keys, this.scroller);
    this.scroller.addEventListener("scroll", () => this.drawFrame());
    new ResizeObserver(() => this.render()).observe(this.scroller);
    this.grid.addEventListener("pointerdown", (e) => this.down(e));
    this.grid.addEventListener("pointermove", (e) => this.move(e));
    this.grid.addEventListener("pointerup", () => this.up());
    this.grid.addEventListener("dblclick", (e) => this.remove(e));
    this.ruler.addEventListener("pointerdown", (e) => {
      const beat = this.beatAt(e.clientX - this.ruler.getBoundingClientRect().left + this.scroller.scrollLeft);
      editor.player.play((beat * 60) / editor.music.bpm);
    });
    this.keys.addEventListener("pointerdown", (e) => {
      const midi = this.pitchAt(e.clientY - this.keys.getBoundingClientRect().top + this.scroller.scrollTop);
      void editor.audition(midi);
    });
    const tick = () => {
      if (editor.player.playing) this.drawFrame();
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }

  private get beats() {
    return this.editor.music.bars * this.editor.music.beats_per_bar;
  }

  private x = (beat: number) => beat * this.editor.zoom;
  private y = (midi: number) => (this.high - midi) * ROW;
  private beatAt = (x: number) => Math.max(0, x / this.editor.zoom);
  private pitchAt = (y: number) => Math.round(this.high - y / ROW + 0.5);
  private snapped = (beat: number) => Math.round(beat / this.editor.snap) * this.editor.snap;

  /** Étendue des hauteurs : la tessiture de l'instrument (et les notes déjà écrites), avec une marge. */
  private range(): [number, number] {
    const { track, instruments } = this.editor;
    if (!track) return [36, 84];
    const info = typeof track.instrument === "string" ? instruments.find((i) => i.id === track.instrument) : null;
    const notes = track.notes.map((n) => midiOf(n.note));
    const lo = Math.min(info ? info.range[0] - track.transpose : 36, ...notes);
    const hi = Math.max(info ? info.range[1] - track.transpose : 84, ...notes);
    return [Math.max(0, lo - 2), Math.min(127, hi + 2)];
  }

  render(): void {
    [this.low, this.high] = this.range();
    if (this.editor.fit && this.scroller.clientWidth > 0) {
      this.editor.fit = false;
      this.editor.zoom = Math.min(96, Math.max(10, Math.floor((this.scroller.clientWidth - 30) / this.beats)));
    }
    const width = this.x(this.beats) + 40;
    const height = (this.high - this.low + 1) * ROW;
    const ratio = window.devicePixelRatio || 1;
    this.grid.style.width = `${width}px`;
    this.grid.style.height = `${height}px`;
    this.grid.width = Math.round(width * ratio);
    this.grid.height = Math.round(height * ratio);
    this.drawGrid();
    // À l'ouverture d'une piste, on se place sur ses notes.
    const key = `${this.editor.doc.name}/${this.editor.selected}`;
    if (key !== this.scrolledTo) {
      this.scrolledTo = key;
      const notes = this.editor.track?.notes.map((n) => midiOf(n.note)) ?? [];
      const center = notes.length ? (Math.min(...notes) + Math.max(...notes)) / 2 : (this.low + this.high) / 2;
      this.scroller.scrollTop = Math.max(0, this.y(center) - this.scroller.clientHeight / 2);
    }
    this.drawFrame();
  }

  /** La règle, le clavier et la tête de lecture suivent le défilement. */
  private drawFrame(): void {
    const ratio = window.devicePixelRatio || 1;
    const sw = this.scroller.clientWidth;
    const sh = this.scroller.clientHeight;
    for (const [canvas, w, h] of [[this.ruler, sw, RULER], [this.keys, KEYS, sh]] as const) {
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
      canvas.width = Math.round(w * ratio);
      canvas.height = Math.round(h * ratio);
    }
    const left = this.scroller.scrollLeft;
    const top = this.scroller.scrollTop;
    const { music, player } = this.editor;

    const r = this.ruler.getContext("2d")!;
    r.setTransform(ratio, 0, 0, ratio, 0, 0);
    r.fillStyle = "#1e1f24";
    r.fillRect(0, 0, sw, RULER);
    r.font = "10.5px -apple-system, system-ui, sans-serif";
    for (let bar = 0; bar <= music.bars; bar++) {
      const x = this.x(bar * music.beats_per_bar) - left;
      if (x < -40 || x > sw) continue;
      r.fillStyle = "rgba(255,255,255,0.25)";
      r.fillRect(Math.round(x), 8, 1, RULER - 8);
      r.fillStyle = "rgba(255,255,255,0.6)";
      if (bar < music.bars) r.fillText(String(bar + 1), x + 4, 13);
    }
    const position = player.position();
    if (position !== null) {
      const x = this.x((position * music.bpm) / 60) - left;
      r.fillStyle = "#ff9a4a";
      r.beginPath();
      r.moveTo(x - 5, RULER - 7);
      r.lineTo(x + 5, RULER - 7);
      r.lineTo(x, RULER);
      r.fill();
    }

    const k = this.keys.getContext("2d")!;
    k.setTransform(ratio, 0, 0, ratio, 0, 0);
    k.clearRect(0, 0, KEYS, sh);
    for (let midi = this.low; midi <= this.high; midi++) {
      const y = this.y(midi) - top;
      if (y < -ROW || y > sh) continue;
      k.fillStyle = isBlackKey(midi) ? "#2a2b31" : "#d9d8de";
      k.fillRect(0, y, KEYS, ROW - 1);
      if (midi % 12 === 0 || midi === this.high) {
        k.fillStyle = isBlackKey(midi) ? "#ddd" : "#333";
        k.font = "9.5px -apple-system, system-ui, sans-serif";
        k.fillText(noteName(midi), 4, y + ROW - 3);
      }
    }
    this.drawPlayhead(position, left);
  }

  private drawPlayhead(position: number | null, left: number): void {
    // La tête de lecture est un trait par-dessus la grille, dessiné à part pour ne pas tout redessiner.
    let head = this.root.querySelector<HTMLElement>(".pr-head");
    if (!head) {
      head = document.createElement("div");
      head.className = "pr-head";
      this.root.append(head);
    }
    head.style.display = position === null ? "none" : "block";
    if (position !== null) head.style.left = `${KEYS + this.x((position * this.editor.music.bpm) / 60) - left}px`;
  }

  private drawGrid(): void {
    const ratio = window.devicePixelRatio || 1;
    const g = this.grid.getContext("2d")!;
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    const w = this.x(this.beats);
    const h = (this.high - this.low + 1) * ROW;
    g.fillStyle = "#16171b";
    g.fillRect(0, 0, w + 40, h);
    for (let midi = this.low; midi <= this.high; midi++) {
      if (isBlackKey(midi)) {
        g.fillStyle = "#121317";
        g.fillRect(0, this.y(midi), w, ROW);
      }
      if (midi % 12 === 0) {
        g.fillStyle = "rgba(255,255,255,0.08)";
        g.fillRect(0, this.y(midi) + ROW - 1, w, 1);
      }
    }
    const { music, snap } = this.editor;
    for (let beat = 0; beat <= this.beats + 1e-6; beat += snap) {
      const bar = Math.abs(beat / music.beats_per_bar - Math.round(beat / music.beats_per_bar)) < 1e-6;
      const whole = Math.abs(beat - Math.round(beat)) < 1e-6;
      g.fillStyle = bar ? "rgba(255,255,255,0.22)" : whole ? "rgba(255,255,255,0.09)" : "rgba(255,255,255,0.035)";
      g.fillRect(Math.round(this.x(beat)), 0, 1, h);
    }
    // Les autres pistes, en filigrane.
    if (this.editor.showOthers) {
      music.tracks.forEach((track, t) => {
        if (t === this.editor.selected) return;
        g.fillStyle = `${layerColor(t)}22`;
        for (const n of [...track.notes.map((n) => ({ ...n, midi: midiOf(n.note) })), ...patternNotes(music, track)]) {
          g.fillRect(this.x(n.at), this.y(n.midi) + 2, Math.max(2, this.x(n.len) - 1), ROW - 4);
        }
      });
    }
    const track = this.editor.track;
    if (!track) return;
    const color = layerColor(this.editor.selected);
    // Notes des motifs : hachurées, non modifiables ici.
    for (const n of patternNotes(music, track)) {
      const x = this.x(n.at);
      const y = this.y(n.midi);
      const wn = Math.max(3, this.x(n.len) - 1);
      g.fillStyle = `${color}55`;
      g.fillRect(x, y + 1, wn, ROW - 2);
      g.save();
      g.beginPath();
      g.rect(x, y + 1, wn, ROW - 2);
      g.clip();
      g.strokeStyle = "rgba(0,0,0,0.35)";
      for (let k = -ROW; k < wn; k += 5) {
        g.beginPath();
        g.moveTo(x + k, y + ROW);
        g.lineTo(x + k + ROW, y);
        g.stroke();
      }
      g.restore();
    }
    track.notes.forEach((n, index) => {
      const x = this.x(n.at);
      const y = this.y(midiOf(n.note));
      const wn = Math.max(3, this.x(n.len) - 1);
      const selected = index === this.editor.note;
      g.globalAlpha = 0.45 + 0.55 * n.vel;
      g.fillStyle = color;
      g.beginPath();
      g.roundRect(x, y + 1, wn, ROW - 2, 2);
      g.fill();
      g.globalAlpha = 1;
      if (selected) {
        g.strokeStyle = "#fff";
        g.lineWidth = 1.5;
        g.stroke();
      }
      // La force, en trait sombre au bas de la note.
      g.fillStyle = "rgba(0,0,0,0.35)";
      g.fillRect(x + 1, y + ROW - 3, Math.max(1, (wn - 2) * n.vel), 1.5);
    });
  }

  // ---------- Gestes ----------

  private point(e: PointerEvent | MouseEvent): { beat: number; midi: number; x: number } {
    const rect = this.grid.getBoundingClientRect();
    const x = e.clientX - rect.left;
    return { beat: this.beatAt(x), midi: this.pitchAt(e.clientY - rect.top), x };
  }

  private hit(beat: number, midi: number, x: number): { index: number; edge: boolean } | null {
    const notes = this.editor.track?.notes ?? [];
    for (let i = notes.length - 1; i >= 0; i--) {
      const n = notes[i];
      if (midiOf(n.note) !== midi || beat < n.at || beat > n.at + n.len) continue;
      return { index: i, edge: this.x(n.at + n.len) - x <= EDGE };
    }
    return null;
  }

  private down(e: PointerEvent): void {
    const { editor } = this;
    if (!editor.track) return;
    this.grid.setPointerCapture(e.pointerId);
    const { beat, midi, x } = this.point(e);
    const hit = this.hit(beat, midi, x);
    if (hit) {
      const note = editor.track.notes[hit.index];
      editor.select(editor.selected, hit.index);
      this.drag = { mode: hit.edge ? "resize" : "move", index: hit.index, dx: beat - note.at, startAt: note.at, pitch: midi };
      editor.gesture(hit.edge ? "Note rallongée" : "Note déplacée", "start");
      if (!hit.edge) void editor.audition(midiOf(note.note), note.vel);
    } else {
      const at = Math.floor(beat / editor.snap) * editor.snap;
      if (at >= this.beats) return;
      editor.gesture(`Note ${noteName(midi)} ajoutée`, "start", (take) => {
        take.music.tracks[editor.selected].notes.push({ at, len: Math.min(this.lastLen, this.beats - at), note: midi, vel: this.lastVel });
      });
      const index = editor.track.notes.length - 1;
      editor.note = index;
      this.drag = { mode: "create", index, dx: 0, startAt: at, pitch: midi };
      void editor.audition(midi, this.lastVel);
    }
    this.drawGrid();
  }

  private move(e: PointerEvent): void {
    const { editor } = this;
    const { beat, midi, x } = this.point(e);
    if (!this.drag) {
      const hit = this.hit(beat, midi, x);
      this.grid.style.cursor = hit ? (hit.edge ? "ew-resize" : "grab") : "crosshair";
      return;
    }
    const d = this.drag;
    editor.gesture("", "move", (take) => {
      const note: Note = take.music.tracks[editor.selected].notes[d.index];
      if (!note) return;
      if (d.mode === "move") {
        note.at = Math.min(this.beats - editor.snap, Math.max(0, this.snapped(beat - d.dx)));
        if (midi !== midiOf(note.note)) {
          note.note = midi;
          void editor.audition(midi, note.vel);
        }
      } else {
        // Créer ou rallonger : la fin suit la souris, au moins un pas de grille.
        const end = Math.max(note.at + editor.snap, this.snapped(beat + editor.snap / 2));
        note.len = Math.min(end, this.beats) - note.at;
      }
    });
    this.drawGrid();
  }

  private up(): void {
    if (!this.drag) return;
    const note = this.editor.track?.notes[this.drag.index];
    if (note) {
      this.lastLen = note.len;
      this.lastVel = note.vel;
    }
    this.drag = null;
    this.editor.gesture("", "end");
  }

  private remove(e: MouseEvent): void {
    const { beat, midi, x } = this.point(e);
    const hit = this.hit(beat, midi, x);
    if (hit) this.deleteNote(hit.index);
  }

  deleteNote(index: number): void {
    const { editor } = this;
    editor.note = null;
    editor.edit("Note supprimée", (take) => void take.music.tracks[editor.selected].notes.splice(index, 1));
  }

  /** Clavier : ↑ ↓ hauteur (⇧ : octave), ← → un pas de grille, Suppr. Rend vrai si la touche a servi. */
  key(e: KeyboardEvent): boolean {
    const { editor } = this;
    const index = editor.note;
    if (index === null || !editor.track?.notes[index]) return false;
    if (e.key === "Delete" || e.key === "Backspace") {
      this.deleteNote(index);
      return true;
    }
    const step = { ArrowUp: [0, e.shiftKey ? 12 : 1], ArrowDown: [0, e.shiftKey ? -12 : -1], ArrowLeft: [-editor.snap, 0], ArrowRight: [editor.snap, 0] }[e.key];
    if (!step) return false;
    editor.edit(step[1] ? "Note transposée" : "Note déplacée", (take) => {
      const note = take.music.tracks[editor.selected].notes[index];
      note.at = Math.min(this.beats - editor.snap, Math.max(0, note.at + step[0]));
      note.note = Math.min(127, Math.max(0, midiOf(note.note) + step[1]));
    });
    const note = editor.track.notes[index];
    if (step[1]) void editor.audition(midiOf(note.note), note.vel);
    return true;
  }

  describe(index: number | null): string {
    const note = index === null ? null : this.editor.track?.notes[index];
    return note ? `${noteName(midiOf(note.note))} (${frenchName(midiOf(note.note))})` : "";
  }
}
