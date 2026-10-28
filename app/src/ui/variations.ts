/**
 * Explorer en écoutant : six versions voisines du son, à comparer à l'oreille. On garde la meilleure, et
 * six nouvelles naissent autour d'elle. On n'a pas besoin de savoir quel réglage fait quoi : on choisit,
 * et l'historique garde chaque pas (⌘Z).
 *
 * `mutate` bouge un peu chaque réglage continu (hauteur, durées, filtres, objets, effets) et, à forte
 * audace, change parfois un choix (matière de l'objet, réverbération). Graine fixe : même graine, mêmes
 * propositions.
 */

import type { Editor } from "../editor";
import { MATERIALS, type BasicMaterial, type Take, clone, defaultReverb } from "../model";
import * as native from "../native";
import { el, numberField } from "./controls";
import { esc, icon } from "./icons";

/** Générateur pseudo-aléatoire à graine (mulberry32). */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const r4 = (v: number) => Number(v.toPrecision(4));

/** Une version voisine de la prise. `audace` : 0 = presque identique, 1 = franchement différente. */
export function mutate(take: Take, audace: number, random: () => number): Take {
  const t = clone(take);
  const bi = () => random() * 2 - 1;
  const a = clamp(audace, 0, 1);
  const times = (v: number) => r4(Math.max(0, v * Math.pow(2, bi() * a * 1.2)));
  const pitchRatio = Math.pow(2, (bi() * a * 7) / 12); // toutes les couches ensemble : l'harmonie reste
  for (const l of t.sound.layers) {
    const own = Math.pow(2, (bi() * a * 2) / 12);
    l.pitch.start = r4(l.pitch.start * pitchRatio * own);
    l.pitch.end = r4(l.pitch.end * pitchRatio * own * Math.pow(2, (bi() * a * 4) / 12));
    l.pitch.time = times(l.pitch.time);
    const e = l.envelope;
    e.attack = times(e.attack);
    e.decay = times(e.decay);
    e.hold = times(e.hold);
    e.release = times(e.release);
    e.sustain = r4(clamp(e.sustain + bi() * a * 0.25, 0, 1));
    l.gain = r4(clamp(l.gain * (1 + bi() * a * 0.5), 0.05, 3));
    if (l.filter) {
      const k = Math.pow(2, bi() * a * 1.6);
      l.filter.cutoff.start = r4(clamp(l.filter.cutoff.start * k, 20, 20000));
      l.filter.cutoff.end = r4(clamp(l.filter.cutoff.end * k * Math.pow(2, bi() * a), 20, 20000));
      l.filter.resonance = r4(clamp(l.filter.resonance + bi() * a * 0.3, 0, 0.95));
    }
    if (l.resonator) {
      l.resonator.decay = r4(clamp(l.resonator.decay * Math.pow(2, bi() * a), 0.05, 8));
      l.resonator.brightness = r4(clamp(l.resonator.brightness + bi() * a * 0.35, 0, 1));
      if (typeof l.resonator.material === "string" && random() < a * 0.15) {
        const choices = MATERIALS.map(([m]) => m).filter((m) => m !== "custom");
        l.resonator.material = choices[Math.floor(random() * choices.length)] as BasicMaterial;
      }
    }
    if (l.vibrato) l.vibrato.depth_semitones = r4(Math.max(0, l.vibrato.depth_semitones * (1 + bi() * a)));
    if (l.unison.voices > 1) l.unison.detune_cents = r4(Math.max(0, l.unison.detune_cents * (1 + bi() * a)));
  }
  const fx = t.sound.effects;
  if (fx.drive !== null) fx.drive = r4(clamp(fx.drive * (1 + bi() * a), 0.1, 6));
  if (fx.echo) fx.echo.mix = r4(clamp(fx.echo.mix + bi() * a * 0.2, 0, 1));
  if (fx.reverb) {
    fx.reverb.mix = r4(clamp(fx.reverb.mix + bi() * a * 0.2, 0, 1));
    fx.reverb.room = r4(clamp(fx.reverb.room + bi() * a * 0.3, 0, 1));
  } else if (random() < a * 0.12) {
    fx.reverb = { ...defaultReverb(), mix: 0.12 };
  }
  return t;
}

interface Proposal {
  letter: string;
  take: Take;
  buffer: AudioBuffer | null;
  error: string | null;
}

let proposals: Proposal[] = [];
let audace = 0.35;
let round = 0;
let busy = false;

export function clearVariations(): void {
  proposals = [];
}

export async function propose(editor: Editor, root: HTMLElement): Promise<void> {
  if (busy) return;
  busy = true;
  round += 1;
  const random = rng(editor.doc.take.seed * 7919 + round * 104729);
  const base = editor.doc.take;
  proposals = "ABCDEF".split("").map((letter) => ({ letter, take: mutate(base, audace, random), buffer: null, error: null }));
  renderVariations(root, editor);
  await Promise.all(
    proposals.map(async (p) => {
      try {
        const rendered = await native.render({ ...p.take, variants: 1 }, 0);
        p.buffer = await editor.player.decode(rendered.wav);
      } catch (error) {
        p.error = error instanceof Error ? error.message : String(error);
      }
    }),
  );
  busy = false;
  renderVariations(root, editor);
}

export function renderVariations(root: HTMLElement, editor: Editor): void {
  root.innerHTML = `
    <header>
      <h3>Explorer en écoutant</h3>
      <span class="block-help">Six versions voisines : écoute-les (clic), garde la meilleure, six autres naissent autour d'elle.</span>
    </header>`;
  const controls = el("div", "variation-controls");
  controls.append(
    numberField({
      label: "Audace",
      value: audace,
      min: 0.05,
      max: 1,
      reset: 0.35,
      hint: "Petite : des nuances. Grande : des sons franchement différents.",
      onChange: (v) => (audace = v),
    }),
  );
  const go = el("button", "button primary", `${icon("dice", 14)} ${proposals.length ? "Six autres" : "Proposer six variations"}`);
  go.disabled = busy;
  go.addEventListener("click", () => void propose(editor, root));
  controls.append(go);
  root.append(controls);
  if (!proposals.length) return;
  const grid = el("div", "variation-grid");
  for (const p of proposals) {
    const tile = el("div", `variation${p.buffer ? "" : " pending"}`);
    tile.innerHTML = `
      <button class="variation-play" title="Écouter">${icon("play", 13)} <b>${p.letter}</b></button>
      <canvas></canvas>
      <button class="button small keep" title="Garder cette version (annulable avec ⌘Z)" ${p.buffer ? "" : "disabled"}>Garder</button>
      ${p.error ? `<span class="variation-error">${esc(p.error)}</span>` : ""}`;
    const canvas = tile.querySelector("canvas")!;
    if (p.buffer) requestAnimationFrame(() => drawMini(canvas, p.buffer!));
    const play = () => p.buffer && editor.player.playBuffer(p.buffer);
    tile.querySelector(".variation-play")!.addEventListener("click", play);
    canvas.addEventListener("click", play);
    tile.querySelector(".keep")!.addEventListener("click", () => {
      editor.edit(`Variation ${p.letter} gardée`, (take) => Object.assign(take, clone(p.take), { variants: take.variants, amount: take.amount, seed: take.seed }));
      void propose(editor, root);
    });
    grid.append(tile);
  }
  root.append(grid);
}

function drawMini(canvas: HTMLCanvasElement, buffer: AudioBuffer): void {
  const ratio = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(canvas.clientWidth * ratio));
  const h = Math.max(1, Math.round(canvas.clientHeight * ratio));
  canvas.width = w;
  canvas.height = h;
  const g = canvas.getContext("2d")!;
  const data = buffer.getChannelData(0);
  g.fillStyle = "#6ebeff";
  for (let x = 0; x < w; x++) {
    let peak = 0;
    const from = Math.floor((x * data.length) / w);
    const to = Math.floor(((x + 1) * data.length) / w);
    for (let i = from; i < to; i++) peak = Math.max(peak, Math.abs(data[i]));
    const bar = Math.max(1, peak * h * 0.9);
    g.fillRect(x, (h - bar) / 2, 1, bar);
  }
}
