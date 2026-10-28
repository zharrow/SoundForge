/**
 * Le son, tel que le moteur le décrit (même JSON que les recettes et que `render_sound`).
 * `normalizeSound` complète ce que le JSON peut omettre, avec les valeurs par défaut du moteur :
 * l'éditeur manipule ainsi toujours un son complet.
 */

export type BasicWaveform = "sine" | "triangle" | "saw" | "white_noise" | "brown_noise";
export interface SampleSource { file: string; reverse: boolean }
export type Waveform = BasicWaveform | { square: { duty: number } } | { sample: SampleSource };
export type FilterMode = "low_pass" | "high_pass" | "band_pass";
export type BasicMaterial = "wood" | "stone" | "metal" | "bell" | "glass" | "bone";
export type Material = BasicMaterial | { custom: { modes: Mode[] } };

export interface Sweep { start: number; end: number; time: number }
export interface Adsr { attack: number; decay: number; sustain: number; hold: number; release: number }
export interface FilterSpec { mode: FilterMode; cutoff: Sweep; resonance: number }
export interface Vibrato { rate_hz: number; depth_semitones: number }
export interface Unison { voices: number; detune_cents: number }
export interface Mode { ratio: number; decay: number; gain: number }
export interface Resonator { material: Material; decay: number; brightness: number; dry: number }
export interface Echo { time: number; feedback: number; mix: number }
export interface Reverb { room: number; damping: number; mix: number }
export interface Effects { drive: number | null; echo: Echo | null; reverb: Reverb | null }

export interface Layer {
  /** Rôle de la couche (« Attaque », « Corps », « Queue »…), pour s'y retrouver ; sans effet sur le son. */
  name: string | null;
  waveform: Waveform;
  pitch: Sweep;
  envelope: Adsr;
  filter: FilterSpec | null;
  vibrato: Vibrato | null;
  unison: Unison;
  gain: number;
  start: number;
  resonator: Resonator | null;
}

export interface Sound { name: string; layers: Layer[]; effects: Effects; peak_db: number }

/** Le son et ses réglages de variantes : ce qu'une recette enregistre. */
export interface Take { sound: Sound; variants: number; amount: number; seed: number }

export const MAX_LAYERS = 16;
/** Hauteur à laquelle un enregistrement est lu à sa vitesse d'origine (moteur : sample::ROOT_HZ). */
export const ROOT_HZ = 440;

export const WAVEFORMS: [string, string][] = [
  ["sine", "Sinus"],
  ["triangle", "Triangle"],
  ["saw", "Dent de scie"],
  ["square", "Carré"],
  ["white_noise", "Bruit blanc"],
  ["brown_noise", "Bruit brun"],
  ["sample", "Enregistrement"],
];

export const FILTER_MODES: [FilterMode, string][] = [
  ["low_pass", "Passe-bas"],
  ["band_pass", "Passe-bande"],
  ["high_pass", "Passe-haut"],
];

export const MATERIALS: [string, string][] = [
  ["wood", "Bois"],
  ["stone", "Pierre"],
  ["metal", "Métal"],
  ["bell", "Cloche"],
  ["glass", "Verre"],
  ["bone", "Os"],
  ["custom", "Modes à la main"],
];

/** Modes des matériaux du moteur (resonator.rs) : point de départ quand on passe en modes à la main. */
export const MATERIAL_MODES: Record<BasicMaterial, [number, number, number][]> = {
  wood: [[1, 0.3, 1], [2.76, 0.12, 0.55], [5.4, 0.06, 0.3], [8.93, 0.035, 0.15]],
  stone: [[1, 0.1, 1], [1.47, 0.08, 0.8], [2.09, 0.06, 0.7], [2.56, 0.05, 0.55], [3.39, 0.04, 0.45], [4.08, 0.03, 0.35], [5.22, 0.025, 0.25], [6.41, 0.02, 0.2]],
  metal: [[1, 1.6, 1], [1.59, 1.3, 0.8], [2.14, 1.1, 0.7], [2.3, 1, 0.6], [2.65, 0.9, 0.55], [2.92, 0.8, 0.5], [3.16, 0.7, 0.45], [3.5, 0.6, 0.4]],
  bell: [[0.5, 4, 0.6], [1, 3, 1], [1.19, 2.4, 0.8], [1.5, 2, 0.5], [2, 1.8, 0.9], [2.52, 1.2, 0.4], [2.66, 1.1, 0.35], [3.01, 0.9, 0.3], [4, 0.6, 0.2]],
  glass: [[1, 1.2, 1], [2.32, 0.9, 0.5], [4.25, 0.6, 0.3], [6.63, 0.4, 0.18]],
  bone: [[1, 0.18, 1], [2.02, 0.09, 0.4], [3.1, 0.05, 0.25], [4.3, 0.03, 0.12]],
};

export function waveformKind(w: Waveform): string {
  return typeof w === "string" ? w : "square" in w ? "square" : "sample";
}

export function sampleOf(w: Waveform): SampleSource | null {
  return typeof w === "object" && "sample" in w ? w.sample : null;
}

/** Hauteur (Hz) <-> transposition d'un enregistrement (demi-tons par rapport à sa vitesse d'origine). */
export const semitonesOf = (hz: number) => 12 * Math.log2(hz / ROOT_HZ);
export const hzOfSemitones = (st: number) => ROOT_HZ * Math.pow(2, st / 12);

/** Une couche qui joue un enregistrement, réglée pour l'entendre en entier. */
export function sampleLayer(file: string, seconds: number): Layer {
  return {
    name: "Voix",
    waveform: { sample: { file, reverse: false } },
    pitch: { start: ROOT_HZ, end: ROOT_HZ, time: 0 },
    envelope: { attack: 0.005, decay: 0, sustain: 1, hold: Math.max(0.05, Number(seconds.toFixed(3))), release: 0.05 },
    filter: null,
    vibrato: null,
    unison: { voices: 1, detune_cents: 0 },
    gain: 1,
    start: 0,
    resonator: null,
  };
}

export function isNoise(w: Waveform): boolean {
  return w === "white_noise" || w === "brown_noise";
}

export function materialKind(m: Material): string {
  return typeof m === "string" ? m : "custom";
}

export function newLayer(): Layer {
  return {
    name: null,
    waveform: "sine",
    pitch: { start: 440, end: 440, time: 0 },
    envelope: { attack: 0.005, decay: 0.25, sustain: 0, hold: 0, release: 0 },
    filter: null,
    vibrato: null,
    unison: { voices: 1, detune_cents: 0 },
    gain: 1,
    start: 0,
    resonator: null,
  };
}

export function newSound(name = "nouveau_son"): Sound {
  return { name, layers: [newLayer()], effects: { drive: null, echo: null, reverb: null }, peak_db: -3 };
}

export function newTake(sound = newSound()): Take {
  return { sound, variants: 1, amount: 0.4, seed: 1 };
}

export const defaultFilter = (): FilterSpec => ({ mode: "low_pass", cutoff: { start: 2000, end: 2000, time: 0 }, resonance: 0.3 });
export const defaultVibrato = (): Vibrato => ({ rate_hz: 6, depth_semitones: 0.3 });
export const defaultResonator = (): Resonator => ({ material: "wood", decay: 1, brightness: 0.5, dry: 0 });
export const defaultEcho = (): Echo => ({ time: 0.12, feedback: 0.3, mix: 0.25 });
export const defaultReverb = (): Reverb => ({ room: 0.4, damping: 0.5, mix: 0.18 });

const num = (value: unknown, fallback: number): number => (typeof value === "number" && Number.isFinite(value) ? value : fallback);
const obj = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;

function sweep(raw: unknown, fallback: number): Sweep {
  const s = obj(raw);
  const start = num(s?.start, fallback);
  return { start, end: num(s?.end, start), time: num(s?.time, 0) };
}

function waveform(raw: unknown): Waveform {
  if (typeof raw === "string" && ["sine", "triangle", "saw", "white_noise", "brown_noise"].includes(raw)) return raw as BasicWaveform;
  const square = obj(obj(raw)?.square);
  if (square) return { square: { duty: num(square.duty, 0.5) } };
  const sample = obj(obj(raw)?.sample);
  if (sample && typeof sample.file === "string") return { sample: { file: sample.file, reverse: sample.reverse === true } };
  throw new Error(`Forme d'onde inconnue : ${JSON.stringify(raw)}.`);
}

function material(raw: unknown): Material {
  if (typeof raw === "string" && raw in MATERIAL_MODES) return raw as BasicMaterial;
  const custom = obj(obj(raw)?.custom);
  if (custom && Array.isArray(custom.modes)) {
    return { custom: { modes: custom.modes.map((m) => ({ ratio: num(obj(m)?.ratio, 1), decay: num(obj(m)?.decay, 0.5), gain: num(obj(m)?.gain, 1) })) } };
  }
  throw new Error(`Matériau inconnu : ${JSON.stringify(raw)}.`);
}

function layer(raw: unknown): Layer {
  const l = obj(raw);
  if (!l) throw new Error("Couche illisible.");
  const env = obj(l.envelope);
  const filter = obj(l.filter);
  const vibrato = obj(l.vibrato);
  const unison = obj(l.unison);
  const resonator = obj(l.resonator);
  return {
    name: typeof l.name === "string" && l.name.trim() ? l.name : null,
    waveform: waveform(l.waveform),
    pitch: sweep(l.pitch, 440),
    envelope: {
      attack: num(env?.attack, 0.005),
      decay: num(env?.decay, 0.2),
      sustain: num(env?.sustain, 0),
      hold: num(env?.hold, 0),
      release: num(env?.release, 0),
    },
    filter: filter
      ? { mode: (["low_pass", "high_pass", "band_pass"].includes(filter.mode as string) ? filter.mode : "low_pass") as FilterMode, cutoff: sweep(filter.cutoff, 2000), resonance: num(filter.resonance, 0.3) }
      : null,
    vibrato: vibrato ? { rate_hz: num(vibrato.rate_hz, 6), depth_semitones: num(vibrato.depth_semitones, 0.3) } : null,
    unison: { voices: Math.max(1, Math.round(num(unison?.voices, 1))), detune_cents: num(unison?.detune_cents, 0) },
    gain: num(l.gain, 1),
    start: num(l.start, 0),
    resonator: resonator
      ? { material: material(resonator.material), decay: num(resonator.decay, 1), brightness: num(resonator.brightness, 0.5), dry: num(resonator.dry, 0) }
      : null,
  };
}

/** Un son reçu de l'extérieur (recette, preset, Claude), complété comme le ferait le moteur. */
export function normalizeSound(raw: unknown): Sound {
  const s = obj(raw);
  if (!s || !Array.isArray(s.layers)) throw new Error("Son illisible : il faut au moins `layers`.");
  const fx = obj(s.effects);
  const echo = obj(fx?.echo);
  const reverb = obj(fx?.reverb);
  return {
    name: typeof s.name === "string" ? s.name : "son",
    layers: s.layers.map(layer),
    effects: {
      drive: typeof fx?.drive === "number" ? fx.drive : null,
      echo: echo ? { time: num(echo.time, 0.12), feedback: num(echo.feedback, 0.3), mix: num(echo.mix, 0.25) } : null,
      reverb: reverb ? { room: num(reverb.room, 0.4), damping: num(reverb.damping, 0.5), mix: num(reverb.mix, 0.18) } : null,
    },
    peak_db: num(s.peak_db, -1),
  };
}

export function normalizeTake(raw: { sound: unknown; variants?: number; amount?: number; seed?: number }): Take {
  return {
    sound: normalizeSound(raw.sound),
    variants: Math.min(16, Math.max(1, Math.round(num(raw.variants, 1)))),
    amount: num(raw.amount, 0.4),
    seed: Math.max(0, Math.round(num(raw.seed, 1))),
  };
}

/** Même son, aux arrondis près : le moteur relit les nombres en simple précision (f32). */
export function sameSound(a: unknown, b: unknown): boolean {
  if (typeof a === "number" && typeof b === "number") return Math.abs(a - b) <= 1e-5 * Math.max(1, Math.abs(a), Math.abs(b));
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return a === b;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => sameSound((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

export function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

/** Nom court d'une couche : sa source, et l'objet qu'elle frappe. */
export function describeLayer(l: Layer): string {
  const sample = sampleOf(l.waveform);
  if (sample) {
    const st = Math.round(semitonesOf(l.pitch.start));
    return `Enregistrement « ${sample.file.replace(/^voix\//, "")} »${sample.reverse ? " à l'envers" : ""}${st ? ` · ${st > 0 ? "+" : ""}${st} demi-tons` : ""}`;
  }
  const kind = waveformKind(l.waveform);
  const source = WAVEFORMS.find(([k]) => k === kind)?.[1] ?? kind;
  const pitch = isNoise(l.waveform)
    ? l.filter
      ? ` · ${FILTER_MODES.find(([m]) => m === l.filter!.mode)?.[1].toLowerCase()} ${formatHz(l.filter.cutoff.start)}`
      : ""
    : ` ${formatHz(l.pitch.start)}${Math.abs(l.pitch.end - l.pitch.start) > 0.5 ? ` → ${formatHz(l.pitch.end)}` : ""}`;
  const object = l.resonator ? ` · ${MATERIALS.find(([m]) => m === materialKind(l.resonator!.material))?.[1].toLowerCase()}` : "";
  return `${source}${pitch}${object}`;
}

export function envelopeDuration(e: Adsr): number {
  return e.attack + e.decay + e.hold + e.release;
}

export function formatHz(hz: number): string {
  return hz >= 1000 ? `${(hz / 1000).toFixed(hz >= 10000 ? 1 : 2).replace(/\.?0+$/, "")} kHz` : `${Math.round(hz)} Hz`;
}

const NOTE_NAMES = ["do", "do♯", "ré", "ré♯", "mi", "fa", "fa♯", "sol", "sol♯", "la", "la♯", "si"];

/** Note la plus proche d'une fréquence (la 440 = la3, notation française), avec l'écart en cents. */
export function noteOf(hz: number): string {
  if (!(hz > 0)) return "";
  const midi = 69 + 12 * Math.log2(hz / 440);
  const rounded = Math.round(midi);
  const cents = Math.round((midi - rounded) * 100);
  const name = `${NOTE_NAMES[((rounded % 12) + 12) % 12]}${Math.floor(rounded / 12) - 2}`;
  return cents === 0 ? name : `${name} ${cents > 0 ? "+" : ""}${cents}`;
}

/** Nom de fichier accepté par le moteur : lettres, chiffres, _ et - (64 au plus). */
export function validName(name: string): boolean {
  return /^[A-Za-z0-9_-]{1,64}$/.test(name);
}
