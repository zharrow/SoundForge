/**
 * Les réglages en mots : six curseurs qui agissent sur tout le son à la fois, nommés par ce qu'on entend.
 * Ce sont des coups de pouce : on tire vers « plus grave » ou « plus aigu », le son suit pendant le geste,
 * puis le curseur revient au centre (la retouche reste, une étape d'historique). On peut ainsi pousser
 * plusieurs fois dans le même sens.
 *
 * Chaque curseur part de l'état du son au début du geste : `apply(base, v)` rend un nouveau son pour une
 * valeur v entre -1 et 1, sans accumuler d'arrondis pendant le glisser.
 */

import type { Editor } from "../editor";
import { type Take, clone, defaultReverb, isNoise, sampleOf } from "../model";
import { el } from "./controls";
import { esc } from "./icons";

export interface Macro {
  id: string;
  left: string;
  right: string;
  hint: string;
  /** Effet entendu pour une valeur v (-1..1), affiché pendant le geste. */
  amount: (v: number) => string;
  apply: (base: Take, v: number) => Take;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const r3 = (v: number) => Number(v.toPrecision(4));

export const MACROS: Macro[] = [
  {
    id: "pitch",
    left: "Plus grave",
    right: "Plus aigu",
    hint: "Toutes les hauteurs bougent ensemble (l'harmonie entre couches reste) ; les filtres suivent à moitié.",
    amount: (v) => `${v >= 0 ? "+" : "−"}${Math.abs(Math.round(v * 12))} demi-tons`,
    apply(base, v) {
      const t = clone(base);
      const ratio = Math.pow(2, Math.round(v * 12) / 12);
      for (const l of t.sound.layers) {
        l.pitch.start = r3(l.pitch.start * ratio);
        l.pitch.end = r3(l.pitch.end * ratio);
        if (l.filter) {
          l.filter.cutoff.start = r3(l.filter.cutoff.start * Math.sqrt(ratio));
          l.filter.cutoff.end = r3(l.filter.cutoff.end * Math.sqrt(ratio));
        }
      }
      return t;
    },
  },
  {
    id: "length",
    left: "Plus court",
    right: "Plus long",
    hint: "Étire ou resserre le son dans le temps : enveloppes, glissements, départs des couches, résonances.",
    amount: (v) => `durée ×${Math.pow(2, v * 1.5).toFixed(2).replace(".", ",")}`,
    apply(base, v) {
      const t = clone(base);
      const k = Math.pow(2, v * 1.5);
      for (const l of t.sound.layers) {
        const e = l.envelope;
        e.attack = r3(e.attack * k);
        e.decay = r3(e.decay * k);
        e.hold = r3(e.hold * k);
        e.release = r3(e.release * k);
        l.start = r3(l.start * k);
        l.pitch.time = r3(l.pitch.time * k);
        if (l.filter) l.filter.cutoff.time = r3(l.filter.cutoff.time * k);
        if (l.resonator) l.resonator.decay = r3(clamp(l.resonator.decay * k, 0.05, 8));
      }
      const reverb = t.sound.effects.reverb;
      if (reverb) reverb.room = r3(clamp(reverb.room * Math.pow(k, 0.3), 0, 1));
      return t;
    },
  },
  {
    id: "bright",
    left: "Plus sombre",
    right: "Plus clair",
    hint: "Ouvre ou ferme les aigus : filtres, brillance des objets frappés, aigus de la réverbération.",
    amount: (v) => (v >= 0 ? "plus clair" : "plus sombre"),
    apply(base, v) {
      const t = clone(base);
      const k = Math.pow(2, v * 2.5);
      for (const l of t.sound.layers) {
        if (l.filter) {
          l.filter.cutoff.start = r3(clamp(l.filter.cutoff.start * k, 20, 20000));
          l.filter.cutoff.end = r3(clamp(l.filter.cutoff.end * k, 20, 20000));
        }
        if (l.resonator) l.resonator.brightness = r3(clamp(l.resonator.brightness + v * 0.4, 0, 1));
      }
      const reverb = t.sound.effects.reverb;
      if (reverb) reverb.damping = r3(clamp(reverb.damping - v * 0.35, 0, 1));
      return t;
    },
  },
  {
    id: "bite",
    left: "Plus doux",
    right: "Plus agressif",
    hint: "Agressif : saturation, attaques plus sèches, filtres plus mordants. Doux : l'inverse.",
    amount: (v) => (v >= 0 ? "plus de mordant" : "plus doux"),
    apply(base, v) {
      const t = clone(base);
      const fx = t.sound.effects;
      const drive = (fx.drive ?? 0) + v * 2.5;
      fx.drive = drive > 0.1 ? r3(Math.min(drive, 6)) : null;
      for (const l of t.sound.layers) {
        const e = l.envelope;
        e.attack = r3(e.attack * Math.pow(2, -v * 1.5));
        if (l.filter) l.filter.resonance = r3(clamp(l.filter.resonance + v * 0.2, 0, 0.95));
      }
      return t;
    },
  },
  {
    id: "space",
    left: "Plus sec",
    right: "Plus lointain",
    hint: "Lointain : plus de réverbération, une pièce plus grande, des aigus amortis. Sec : on retire l'espace.",
    amount: (v) => (v >= 0 ? "plus d'espace" : "plus sec"),
    apply(base, v) {
      const t = clone(base);
      const fx = t.sound.effects;
      if (v > 0) {
        const reverb = fx.reverb ?? { ...defaultReverb(), mix: 0 };
        reverb.mix = r3(clamp(reverb.mix + v * 0.4, 0, 1));
        reverb.room = r3(clamp(reverb.room + v * 0.35, 0, 1));
        reverb.damping = r3(clamp(reverb.damping + v * 0.2, 0, 1));
        fx.reverb = reverb;
      } else {
        if (fx.reverb) {
          const mix = fx.reverb.mix * (1 + v);
          fx.reverb = mix < 0.01 ? null : { ...fx.reverb, mix: r3(mix) };
        }
        if (fx.echo) {
          const mix = fx.echo.mix * (1 + v);
          fx.echo = mix < 0.01 ? null : { ...fx.echo, mix: r3(mix) };
        }
      }
      return t;
    },
  },
  {
    id: "texture",
    left: "Plus plein",
    right: "Plus soufflé",
    hint: "Équilibre entre les couches qui ont une note (pleines) et les bruits (souffle, grain).",
    amount: (v) => (v >= 0 ? "bruits plus forts" : "notes plus fortes"),
    apply(base, v) {
      const t = clone(base);
      for (const l of t.sound.layers) {
        const breath = isNoise(l.waveform) && !l.resonator && !sampleOf(l.waveform);
        l.gain = r3(clamp(l.gain * Math.pow(2, (breath ? 1 : -1) * v * 1.2), 0, 4));
      }
      return t;
    },
  },
];

export function renderMacros(root: HTMLElement, editor: Editor): void {
  root.innerHTML = `<header><h3>Réglages en mots</h3><span class="block-help">Tire vers le mot : le son suit, puis le curseur revient au centre. On peut pousser plusieurs fois.</span></header>`;
  const grid = el("div", "macro-grid");
  for (const macro of MACROS) grid.append(macroSlider(editor, macro));
  root.append(grid);
}

function macroSlider(editor: Editor, macro: Macro): HTMLElement {
  const row = el("div", "macro");
  row.title = macro.hint;
  row.innerHTML = `
    <span class="macro-left">${esc(macro.left)}</span>
    <div class="macro-track"><div class="macro-fill"></div><div class="macro-knob"></div></div>
    <span class="macro-right">${esc(macro.right)}</span>
    <span class="macro-amount"></span>`;
  const track = row.querySelector<HTMLElement>(".macro-track")!;
  const knob = row.querySelector<HTMLElement>(".macro-knob")!;
  const fill = row.querySelector<HTMLElement>(".macro-fill")!;
  const amount = row.querySelector<HTMLElement>(".macro-amount")!;
  const show = (v: number) => {
    knob.style.left = `${50 + v * 50}%`;
    fill.style.left = `${50 + Math.min(0, v) * 50}%`;
    fill.style.width = `${Math.abs(v) * 50}%`;
    amount.textContent = v === 0 ? "" : macro.amount(v);
    row.classList.toggle("active", v !== 0);
  };
  show(0);
  const valueAt = (x: number) => {
    const rect = track.getBoundingClientRect();
    return Math.min(1, Math.max(-1, ((x - rect.left) / rect.width) * 2 - 1));
  };
  track.addEventListener("pointerdown", (e) => {
    track.setPointerCapture(e.pointerId);
    const base = clone(editor.doc.take);
    const rect = track.getBoundingClientRect();
    // Un clic vaut un petit coup de pouce dans son sens ; un glisser suit la souris à partir de là.
    const clicked = valueAt(e.clientX);
    const from = Math.sign(clicked) * Math.min(Math.abs(clicked), 0.25);
    const label = (v: number) => `${v < 0 ? macro.left : macro.right} (${macro.amount(v)})`;
    let v = from;
    const apply = (phase: "start" | "move") => {
      show(v);
      editor.gesture(label(v), phase, (take) => Object.assign(take, macro.apply(base, v)));
    };
    apply("start");
    track.onpointermove = (m) => {
      v = Math.min(1, Math.max(-1, from + ((m.clientX - e.clientX) / rect.width) * 2));
      apply("move");
    };
    track.onpointerup = () => {
      track.onpointermove = null;
      track.onpointerup = null;
      editor.doc.relabel(label(v));
      editor.gesture(label(v), "end");
      show(0);
    };
  });
  return row;
}
