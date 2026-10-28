import { beforeEach, describe, expect, it, vi } from "vitest";
import * as m from "./music-model";

const rendered: { music: m.Music; seed: number }[] = [];
vi.mock("./native", () => ({
  renderMusicTake: vi.fn(async (music: m.Music, seed: number) => {
    rendered.push({ music: JSON.parse(JSON.stringify(music)), seed });
    const stems = [...new Set(music.tracks.map((t) => t.stem || t.name))].map((s) => [s, -24]);
    return { stems, duration: 4, frames: 176400, rendered: music.tracks.length, spectrum: "" };
  }),
  musicStem: vi.fn(async () => new ArrayBuffer(8)),
  previewNote: vi.fn(async () => new ArrayBuffer(8)),
  musicLibrary: vi.fn(async () => ({ pieces: [], instruments: [], catalogue_error: null })),
}));

const { MusicEditor } = await import("./music-editor");

function music(): m.Music {
  return m.normalizeMusic({
    name: "essai",
    bpm: 120,
    beats_per_bar: 4,
    bars: 2,
    looping: true,
    peak_db: -1,
    reverb: null,
    patterns: { motif: [{ at: 0, len: 1, note: "D4", vel: 0.8 }, { at: 1, len: 1, note: "F4", vel: 0.5 }] },
    tracks: [
      { ...m.newTrack("harpe", "harpe", "ambience"), notes: [{ at: 0, len: 2, note: "A3", vel: 0.6 }] },
      { ...m.newTrack("piano", "piano", "melody"), play: [{ pattern: "motif", at: 0, times: 2, transpose: 12, vel: 0.5 }] },
    ],
  } as m.Music);
}

beforeEach(() => {
  rendered.length = 0;
  vi.stubGlobal("window", { setTimeout, clearTimeout });
});

describe("modèle de musique", () => {
  it("lit les noms de notes et les écrit dans les deux notations", () => {
    expect(m.midiOf("A4")).toBe(69);
    expect(m.midiOf("Bb3")).toBe(58);
    expect(m.midiOf("C#2")).toBe(37);
    expect(m.midiOf(60)).toBe(60);
    expect(m.noteName(62)).toBe("D4");
    expect(m.frenchName(62)).toBe("ré3");
    expect(() => m.midiOf("H2")).toThrow(/illisible/);
  });

  it("déroule les motifs comme le moteur, puis les détache en notes modifiables", () => {
    const piece = music();
    const piano = piece.tracks[1];
    const notes = m.patternNotes(piece, piano);
    expect(notes.map((n) => [n.at, n.midi, n.vel])).toEqual([[0, 74, 0.4], [1, 77, 0.25], [4, 74, 0.4], [5, 77, 0.25]]);
    m.detachPatterns(piece, piano);
    expect(piano.play).toEqual([]);
    expect(piano.notes.map((n) => [n.at, n.note])).toEqual([[0, 74], [1, 77], [4, 74], [5, 77]]);
  });

  it("normalise un morceau venu du disque : notes en MIDI, valeurs par défaut", () => {
    const piece = music();
    expect(piece.tracks[0].notes[0].note).toBe(57);
    expect(piece.tracks[0].humanize).toBe(0.3);
    expect(m.stemOf(piece.tracks[1])).toBe("melody");
  });

  it("signale les notes hors tessiture (le moteur les refuserait)", () => {
    const piece = music();
    const info: m.InstrumentInfo[] = [{ id: "harpe", name: "Harpe", family: "cordes", kind: "frappe", range: [60, 90], layers: 1, kit: [] }];
    expect(m.outOfRange(piece.tracks[0], info)).toEqual([57]);
    piece.tracks[0].transpose = 12;
    expect(m.outOfRange(piece.tracks[0], info)).toEqual([]);
  });
});

describe("éditeur de musique", () => {
  it("une retouche s'annule, et relance un rendu", async () => {
    const editor = new MusicEditor();
    editor.open({ music: music(), seed: 3 }, "essai", "Ouverture");
    await editor.settled();
    editor.edit("Tempo", (take) => (take.music.bpm = 90));
    await editor.settled();
    expect(rendered.at(-1)!.music.bpm).toBe(90);
    expect(rendered.at(-1)!.seed).toBe(3);
    editor.undo();
    expect(editor.music.bpm).toBe(120);
    expect(editor.doc.dirty).toBe(false);
  });

  it("couper une piste la retire du rendu, sans toucher à la partition", async () => {
    const editor = new MusicEditor();
    editor.open({ music: music(), seed: 1 }, "essai", "Ouverture");
    await editor.settled();
    editor.toggleMute(0);
    await editor.settled();
    expect(rendered.at(-1)!.music.tracks.map((t) => t.name)).toEqual(["piano"]);
    expect(editor.music.tracks).toHaveLength(2);
    expect(editor.doc.dirty).toBe(false);
    expect(editor.stems).toEqual(["ambience", "melody"]);
  });
});
