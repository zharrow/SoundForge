/**
 * Le morceau, tel que le moteur le décrit (même JSON que render_music et que les recettes de musique/).
 * Les notes sont toujours tenues en MIDI dans l'éditeur ; le moteur accepte aussi « D4 ».
 */

import type { Sound } from "./model";

export type Voice = string | { synth: Sound };

export interface Note {
  at: number;
  len: number;
  note: number | string;
  vel: number;
}

export interface PatternUse {
  pattern: string;
  at: number;
  transpose?: number;
  times?: number;
  every?: number | null;
  vel?: number;
}

export interface Track {
  name: string;
  stem?: string | null;
  instrument: Voice;
  volume_db: number;
  pan: number;
  reverb: number;
  transpose: number;
  humanize: number;
  notes: Note[];
  play: PatternUse[];
}

export interface Reverb {
  room: number;
  damping: number;
  mix: number;
}

export interface Music {
  name: string;
  bpm: number;
  beats_per_bar: number;
  bars: number;
  looping: boolean;
  patterns: Record<string, Note[]>;
  tracks: Track[];
  reverb: Reverb | null;
  peak_db: number;
}

/** Ce qu'enregistre une recette de morceau : la partition et sa graine. */
export interface MusicTake {
  music: Music;
  seed: number;
}

export interface InstrumentInfo {
  id: string;
  name: string;
  family: string;
  kind: "tenu" | "frappe";
  range: [number, number];
  layers: number;
  kit: number[];
}

/** Les stems qu'un jeu fait entendre, du plus calme au plus intense (direction audio d'Eku). */
export const STEMS = ["ambience", "melody", "tension", "combat"];

export const STEM_NAMES: Record<string, string> = {
  ambience: "Ambiance",
  melody: "Mélodie",
  tension: "Tension",
  combat: "Combat",
};

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FRENCH = ["do", "do♯", "ré", "ré♯", "mi", "fa", "fa♯", "sol", "sol♯", "la", "la♯", "si"];

export function midiOf(note: number | string): number {
  if (typeof note === "number") return note;
  const m = /^([A-Ga-g])([#b]?)(-?\d)$/.exec(note.trim());
  if (!m) throw new Error(`Note illisible : « ${note} »`);
  const base = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }[m[1].toUpperCase() as "C"];
  return (Number(m[3]) + 1) * 12 + base + (m[2] === "#" ? 1 : m[2] === "b" ? -1 : 0);
}

/** « D4 » (notation internationale, A4 = 440 Hz). */
export const noteName = (midi: number) => `${NAMES[((midi % 12) + 12) % 12]}${Math.floor(midi / 12) - 1}`;
/** « ré3 » (notation française, la3 = 440 Hz). */
export const frenchName = (midi: number) => `${FRENCH[((midi % 12) + 12) % 12]}${Math.floor(midi / 12) - 2}`;
export const isBlackKey = (midi: number) => [1, 3, 6, 8, 10].includes(((midi % 12) + 12) % 12);

export const stemOf = (track: Track) => track.stem || track.name;

/** Les notes qu'un motif fait jouer à la piste (lecture seule dans l'éditeur), avec leur motif d'origine. */
export function patternNotes(music: Music, track: Track): (Note & { midi: number; pattern: string })[] {
  const out: (Note & { midi: number; pattern: string })[] = [];
  for (const use of track.play ?? []) {
    const pattern = music.patterns[use.pattern];
    if (!pattern) continue;
    const span = pattern.reduce((m, n) => Math.max(m, n.at + n.len), 0);
    const every = use.every ?? Math.max(Math.ceil(span / music.beats_per_bar) * music.beats_per_bar, music.beats_per_bar);
    for (let k = 0; k < (use.times ?? 1); k++) {
      for (const n of pattern) {
        out.push({
          ...n,
          at: use.at + k * every + n.at,
          midi: midiOf(n.note) + (use.transpose ?? 0),
          vel: Math.min(1, n.vel * (use.vel ?? 1)),
          pattern: use.pattern,
        });
      }
    }
  }
  return out;
}

/** Les motifs de la piste deviennent ses propres notes : on peut alors les retoucher une à une. */
export function detachPatterns(music: Music, track: Track): void {
  for (const n of patternNotes(music, track)) track.notes.push({ at: n.at, len: n.len, note: n.midi, vel: Number(n.vel.toFixed(3)) });
  track.play = [];
}

export function normalizeMusic(raw: Music): Music {
  return {
    name: raw.name ?? "morceau",
    bpm: raw.bpm ?? 100,
    beats_per_bar: raw.beats_per_bar ?? 4,
    bars: raw.bars ?? 8,
    looping: raw.looping ?? true,
    patterns: raw.patterns ?? {},
    reverb: raw.reverb ?? null,
    peak_db: raw.peak_db ?? -1,
    tracks: (raw.tracks ?? []).map((t) => ({
      name: t.name,
      stem: t.stem ?? null,
      instrument: t.instrument,
      volume_db: t.volume_db ?? 0,
      pan: t.pan ?? 0,
      reverb: t.reverb ?? 0.25,
      transpose: t.transpose ?? 0,
      humanize: t.humanize ?? 0.3,
      notes: (t.notes ?? []).map((n) => ({ at: n.at, len: n.len, note: midiOf(n.note), vel: n.vel ?? 0.7 })),
      play: t.play ?? [],
    })),
  };
}

export function newTrack(name: string, instrument: Voice, stem: string): Track {
  return { name, stem, instrument, volume_db: 0, pan: 0, reverb: 0.3, transpose: 0, humanize: 0.3, notes: [], play: [] };
}

export function newMusic(name = "nouveau_morceau"): Music {
  return {
    name,
    bpm: 90,
    beats_per_bar: 4,
    bars: 8,
    looping: true,
    patterns: {},
    reverb: { room: 0.6, damping: 0.5, mix: 0.25 },
    peak_db: -1.5,
    tracks: [newTrack("piano", "piano", "melody")],
  };
}

export function instrumentLabel(voice: Voice, instruments: InstrumentInfo[]): string {
  if (typeof voice === "object") return `Son de synthèse « ${voice.synth.name} »`;
  return instruments.find((i) => i.id === voice)?.name ?? voice;
}

/** Une piste a-t-elle des notes hors de la tessiture de son instrument ? (le moteur les refuserait) */
export function outOfRange(track: Track, instruments: InstrumentInfo[]): number[] {
  if (typeof track.instrument !== "string") return [];
  const info = instruments.find((i) => i.id === track.instrument);
  if (!info) return [];
  return track.notes.map((n) => midiOf(n.note) + track.transpose).filter((m) => m < info.range[0] || m > info.range[1]);
}
