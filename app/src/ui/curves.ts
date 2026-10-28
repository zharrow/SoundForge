/**
 * Les réglages qu'on dessine, comme dans Pixel Forge : on attrape un point et on le tire.
 * - L'enveloppe : quatre points (sommet de l'attaque, fin du déclin, fin de la tenue, fin de l'extinction).
 * - Un balayage (hauteur, coupure du filtre) : le point de départ monte ou descend, le point d'arrivée se
 *   déplace aussi dans le temps. L'axe vertical est logarithmique, comme l'oreille.
 * Les courbes sont celles du moteur (envelope.rs) : ce qu'on voit est ce qu'on entend.
 * Pendant un glisser, l'échelle du temps reste fixe, sinon le point fuirait sous la souris.
 */

import type { Adsr, Sweep } from "../model";
import type { Phase } from "./controls";
import { formatNumber } from "./controls";

const HIT = 11; // px

/** Volume de l'enveloppe à l'instant t, comme le moteur. */
export function envelopeAt(e: Adsr, t: number): number {
  if (t < e.attack) return t / e.attack;
  t -= e.attack;
  if (t < e.decay) {
    const r = 1 - t / e.decay;
    return e.sustain + (1 - e.sustain) * r * r;
  }
  t -= e.decay;
  if (t < e.hold) return e.sustain;
  t -= e.hold;
  if (t < e.release) {
    const r = 1 - t / e.release;
    return e.sustain * r * r;
  }
  return 0;
}

export function sweepAt(s: Sweep, t: number): number {
  const progress = s.time > 0 ? Math.min(1, Math.max(0, t / s.time)) : 1;
  const start = Math.max(s.start, 1e-3);
  return start * Math.pow(Math.max(s.end, 1e-3) / start, progress);
}

interface Point {
  x: number;
  y: number;
  label: string;
}

/** Toile qui se redessine à sa taille réelle (écran Retina compris) et suit la souris. */
abstract class CurveCanvas {
  readonly canvas = document.createElement("canvas");
  protected span = 1; // s visibles
  protected dragging = -1;
  protected hover = -1;
  private observer: ResizeObserver;

  constructor(
    className: string,
    protected color: string,
  ) {
    this.canvas.className = `curve ${className}`;
    this.observer = new ResizeObserver(() => this.draw());
    this.observer.observe(this.canvas);
    this.canvas.addEventListener("pointerdown", (e) => this.down(e));
    this.canvas.addEventListener("pointermove", (e) => this.move(e));
    this.canvas.addEventListener("pointerleave", () => {
      if (this.dragging < 0) {
        this.hover = -1;
        this.draw();
      }
    });
  }

  protected get w(): number {
    return this.canvas.clientWidth;
  }

  protected get h(): number {
    return this.canvas.clientHeight;
  }

  protected abstract points(): Point[];
  protected abstract computeSpan(): number;
  protected abstract dragTo(index: number, t: number, level: number, phase: Phase): void;
  protected abstract paint(g: CanvasRenderingContext2D): void;

  protected readonly pad = { left: 8, right: 10, top: 10, bottom: 10 };
  protected tx = (t: number) => this.pad.left + ((this.w - this.pad.left - this.pad.right) * t) / this.span;
  protected ty = (level: number) => this.h - this.pad.bottom - (this.h - this.pad.top - this.pad.bottom) * level;
  protected toT = (x: number) => Math.max(0, ((x - this.pad.left) * this.span) / (this.w - this.pad.left - this.pad.right));
  protected toLevel = (y: number) => Math.min(1, Math.max(0, (this.h - this.pad.bottom - y) / (this.h - this.pad.top - this.pad.bottom)));

  private hit(e: PointerEvent): number {
    const rect = this.canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    // À égalité (points superposés), le premier gagne : c'est le plus utile (le déclin d'un son percussif).
    let best = -1;
    let bestDistance = HIT;
    this.points().forEach((p, i) => {
      const d = Math.hypot(this.tx(p.x) - x, this.ty(p.y) - y);
      if (d < bestDistance - 0.5) {
        best = i;
        bestDistance = d;
      }
    });
    return best;
  }

  private down(e: PointerEvent): void {
    const index = this.hit(e);
    if (index < 0) return;
    this.canvas.setPointerCapture(e.pointerId);
    this.dragging = index;
    this.drag(e, "start");
    const up = (u: PointerEvent) => {
      this.drag(u, "end");
      this.dragging = -1;
      this.span = this.computeSpan();
      this.canvas.removeEventListener("pointerup", up);
      this.draw();
    };
    this.canvas.addEventListener("pointerup", up);
  }

  private move(e: PointerEvent): void {
    if (this.dragging >= 0) return this.drag(e, "move");
    const index = this.hit(e);
    if (index !== this.hover) {
      this.hover = index;
      this.canvas.style.cursor = index >= 0 ? "grab" : "default";
      this.draw();
    }
  }

  private drag(e: PointerEvent, phase: Phase): void {
    const rect = this.canvas.getBoundingClientRect();
    this.dragTo(this.dragging, this.toT(e.clientX - rect.left), this.toLevel(e.clientY - rect.top), phase);
    this.draw();
  }

  draw(): void {
    const ratio = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(this.w * ratio));
    const h = Math.max(1, Math.round(this.h * ratio));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    if (this.dragging < 0) this.span = this.computeSpan();
    const g = this.canvas.getContext("2d")!;
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, this.w, this.h);
    this.paint(g);
    // Les poignées, et l'étiquette de celle qu'on vise ou qu'on tire.
    const active = this.dragging >= 0 ? this.dragging : this.hover;
    this.points().forEach((p, i) => {
      g.beginPath();
      g.arc(this.tx(p.x), this.ty(p.y), i === active ? 6 : 4.5, 0, Math.PI * 2);
      g.fillStyle = i === active ? "#fff" : this.color;
      g.fill();
      g.lineWidth = 2;
      g.strokeStyle = "#121317";
      g.stroke();
    });
    if (active >= 0) {
      const p = this.points()[active];
      g.font = "11px -apple-system, system-ui, sans-serif";
      g.fillStyle = "rgba(255,255,255,0.85)";
      const x = Math.min(this.tx(p.x) + 9, this.w - g.measureText(p.label).width - 4);
      g.fillText(p.label, x, Math.max(12, this.ty(p.y) - 8));
    }
  }

  dispose(): void {
    this.observer.disconnect();
  }
}

export class EnvelopeEditor extends CurveCanvas {
  constructor(
    private get: () => Adsr,
    private onChange: (env: Adsr, phase: Phase) => void,
    color: string,
  ) {
    super("env", color);
  }

  protected computeSpan(): number {
    const e = this.get();
    return Math.max(0.02, e.attack + e.decay + e.hold + e.release) * 1.15;
  }

  protected points(): Point[] {
    const e = this.get();
    const s = (v: number) => `${formatNumber(v)} s`;
    return [
      { x: e.attack, y: 1, label: `Attaque ${s(e.attack)}` },
      { x: e.attack + e.decay, y: e.sustain, label: `Déclin ${s(e.decay)} · palier ${formatNumber(e.sustain)}` },
      { x: e.attack + e.decay + e.hold, y: e.sustain, label: `Tenue ${s(e.hold)}` },
      { x: e.attack + e.decay + e.hold + e.release, y: 0, label: `Extinction ${s(e.release)}` },
    ];
  }

  protected dragTo(index: number, t: number, level: number, phase: Phase): void {
    const e = { ...this.get() };
    const round = (v: number) => Number(Math.max(0, v).toFixed(v < 0.1 ? 4 : 3));
    if (index === 0) e.attack = round(t);
    else if (index === 1) {
      e.decay = round(t - e.attack);
      e.sustain = Number(level.toFixed(2));
    } else if (index === 2) {
      e.hold = round(t - e.attack - e.decay);
      e.sustain = Number(level.toFixed(2));
    } else e.release = round(t - e.attack - e.decay - e.hold);
    this.onChange(e, phase);
  }

  protected paint(g: CanvasRenderingContext2D): void {
    const e = this.get();
    const total = e.attack + e.decay + e.hold + e.release;
    g.fillStyle = "#121317";
    g.fillRect(0, 0, this.w, this.h);
    g.beginPath();
    const steps = 160;
    for (let i = 0; i <= steps; i++) {
      const t = (total * i) / steps;
      const x = this.tx(t);
      const y = this.ty(envelopeAt(e, t));
      if (i) g.lineTo(x, y);
      else g.moveTo(x, y);
    }
    g.lineTo(this.tx(total), this.ty(0));
    g.lineTo(this.tx(0), this.ty(0));
    g.closePath();
    g.fillStyle = `${this.color}2a`;
    g.fill();
    g.beginPath();
    for (let i = 0; i <= steps; i++) {
      const t = (total * i) / steps;
      if (i) g.lineTo(this.tx(t), this.ty(envelopeAt(e, t)));
      else g.moveTo(this.tx(t), this.ty(envelopeAt(e, t)));
    }
    g.strokeStyle = this.color;
    g.lineWidth = 2;
    g.stroke();
    g.fillStyle = "rgba(255,255,255,0.4)";
    g.font = "10.5px -apple-system, system-ui, sans-serif";
    g.textAlign = "right";
    g.fillText(`${formatNumber(total)} s`, this.w - 6, 12);
    g.textAlign = "left";
  }
}

export interface SweepAxis {
  min: number;
  max: number;
  /** Texte d'une valeur (« 440 Hz · la3 », « +5 demi-tons »). */
  label: (v: number) => string;
  /** Lignes repères. */
  grid: number[];
}

export class SweepEditor extends CurveCanvas {
  constructor(
    private get: () => { sweep: Sweep; length: number },
    private axis: SweepAxis,
    private onChange: (sweep: Sweep, phase: Phase) => void,
    color: string,
  ) {
    super("sweep", color);
  }

  private level = (v: number) => Math.log(Math.max(v, 1e-3) / this.axis.min) / Math.log(this.axis.max / this.axis.min);
  private value = (level: number) => this.axis.min * Math.pow(this.axis.max / this.axis.min, level);

  protected computeSpan(): number {
    const { sweep, length } = this.get();
    return Math.max(0.05, length, sweep.time * 1.25);
  }

  protected points(): Point[] {
    const { sweep } = this.get();
    return [
      { x: 0, y: clamp01(this.level(sweep.start)), label: `Départ ${this.axis.label(sweep.start)}` },
      { x: sweep.time, y: clamp01(this.level(sweep.end)), label: `Arrivée ${this.axis.label(sweep.end)} en ${formatNumber(sweep.time)} s` },
    ];
  }

  protected dragTo(index: number, t: number, level: number, phase: Phase): void {
    const s = { ...this.get().sweep };
    const v = Number(this.value(level).toPrecision(4));
    if (index === 0) {
      // Sans glissement, départ et arrivée bougent ensemble : c'est la hauteur de la couche.
      if (s.time === 0 && s.start === s.end) s.end = v;
      s.start = v;
    } else {
      s.end = v;
      s.time = Number(Math.max(0, t).toFixed(3));
    }
    this.onChange(s, phase);
  }

  protected paint(g: CanvasRenderingContext2D): void {
    const { sweep } = this.get();
    g.fillStyle = "#121317";
    g.fillRect(0, 0, this.w, this.h);
    g.strokeStyle = "rgba(255,255,255,0.07)";
    g.fillStyle = "rgba(255,255,255,0.3)";
    g.font = "9.5px -apple-system, system-ui, sans-serif";
    for (const v of this.axis.grid) {
      const y = this.ty(this.level(v));
      g.beginPath();
      g.moveTo(this.pad.left, y);
      g.lineTo(this.w - this.pad.right, y);
      g.stroke();
      g.fillText(this.axis.label(v), this.w - this.pad.right - g.measureText(this.axis.label(v)).width, y - 2);
    }
    g.beginPath();
    const steps = 120;
    for (let i = 0; i <= steps; i++) {
      const t = (this.span * i) / steps;
      const y = this.ty(clamp01(this.level(sweepAt(sweep, t))));
      if (i) g.lineTo(this.tx(t), y);
      else g.moveTo(this.tx(t), y);
    }
    g.strokeStyle = this.color;
    g.lineWidth = 2;
    g.stroke();
  }
}

function clamp01(v: number): number {
  return Math.min(1, Math.max(0, v));
}
