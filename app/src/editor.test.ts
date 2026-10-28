import { beforeEach, describe, expect, it, vi } from "vitest";

// Le côté natif est simulé : un rendu factice, un dossier en mémoire.
const disk = new Map<string, { sound: unknown; variants: number; amount: number; seed: number; preset: string | null }>();
vi.mock("./native", () => ({
  isNative: () => false,
  render: vi.fn(async (take: { variants: number }, variant: number, overview = false) => ({
    wav: "",
    spectrum: "",
    overview: overview ? "PNG" : null,
    duration: 0.5,
    peak_db: -1,
    rms_db: -18,
    bands: [["basses", 1]],
    layers: [[0, 0.5]],
    count: Math.max(1, take.variants),
    suffix: take.variants > 1 ? `_0${variant + 1}` : "",
  })),
  library: vi.fn(async () => ({ folder: "/tmp/sfx", sounds: [...disk.keys()].map((name) => ({ name, editable: true, variants: 1, files: [], preset: null, generator: "t", modified: 0 })), presets: [] })),
  loadRecipe: vi.fn(async (name: string) => ({ name, generator: "t", ...disk.get(name)! })),
  preset: vi.fn(async () => ({ name: "jump", layers: [{ waveform: "triangle", pitch: { start: 260, end: 620, time: 0.12 }, envelope: { attack: 0.004, decay: 0.14 } }] })),
  save: vi.fn(async (name: string, take: { sound: unknown; variants: number; amount: number; seed: number }, preset: string | null) => {
    disk.set(name, { ...JSON.parse(JSON.stringify(take)), preset });
    return { files: [`${name}.wav`] };
  }),
  exists: vi.fn(async (name: string) => disk.has(name)),
  reveal: vi.fn(),
  confirm: vi.fn(async () => true),
  alertError: vi.fn(),
  setTitle: vi.fn(),
}));

// WebAudio n'existe pas sous Node : le lecteur est réduit à ce que l'éditeur appelle.
vi.mock("./audio", () => ({
  Player: class {
    buffer = {};
    loop = false;
    playing = false;
    onState = () => {};
    load = async () => ({});
    play() {
      this.playing = true;
    }
    stop() {
      this.playing = false;
    }
    toggle() {}
    position = () => null;
  },
  peaks: () => new Float32Array(),
}));

const { SoundDoc } = await import("./doc");
const { Editor } = await import("./editor");
const { createHandlers } = await import("./bridge");
const { createCommands } = await import("./commands");
const model = await import("./model");

const status = { show: vi.fn(), render: vi.fn() } as never;

function setup() {
  const editor = new Editor();
  const commands = createCommands(editor, status, async () => "nouveau");
  const notify = vi.fn();
  return { editor, commands, rpc: createHandlers(editor, commands, notify), notify };
}

beforeEach(() => {
  disk.clear();
  vi.stubGlobal("window", { setTimeout, clearTimeout });
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {} });
});

describe("historique", () => {
  it("un geste continu ne fait qu'une étape, annulable et rétablissable", () => {
    const doc = new SoundDoc(model.newTake(), "son", null);
    doc.begin("Attaque");
    for (const v of [0.01, 0.02, 0.05]) doc.take.sound.layers[0].envelope.attack = v;
    expect(doc.commit()).toBe(true);
    expect(doc.steps.map((s) => s.label)).toEqual(["Ouverture", "Attaque"]);
    expect(doc.dirty).toBe(true);
    doc.undo();
    expect(doc.take.sound.layers[0].envelope.attack).toBe(0.005);
    expect(doc.dirty).toBe(false);
    doc.redo();
    expect(doc.take.sound.layers[0].envelope.attack).toBe(0.05);
  });

  it("un geste qui ne change rien n'ajoute pas d'étape ; une nouvelle retouche efface le futur", () => {
    const doc = new SoundDoc(model.newTake(), null, null);
    doc.begin("Rien");
    expect(doc.commit()).toBe(false);
    doc.edit("A", (t) => (t.seed = 2));
    doc.edit("B", (t) => (t.seed = 3));
    doc.undo();
    doc.edit("C", (t) => (t.seed = 4));
    expect(doc.steps.map((s) => s.label)).toEqual(["Ouverture", "A", "C"]);
    expect(doc.canRedo).toBe(false);
  });

  it("revenir à la version enregistrée est une étape annulable", () => {
    const doc = new SoundDoc(model.newTake(), "son", null);
    doc.edit("Graine", (t) => (t.seed = 9));
    expect(doc.revert()).toBe(true);
    expect(doc.take.seed).toBe(1);
    expect(doc.dirty).toBe(false);
    doc.undo();
    expect(doc.take.seed).toBe(9);
  });
});

describe("modèle", () => {
  it("complète un son incomplet avec les valeurs par défaut du moteur", () => {
    const sound = model.normalizeSound({ name: "x", layers: [{ waveform: "white_noise", pitch: { start: 1, end: 1, time: 0 }, envelope: { attack: 0, decay: 0.1 } }] });
    expect(sound.peak_db).toBe(-1);
    expect(sound.layers[0]).toMatchObject({ gain: 1, start: 0, filter: null, resonator: null, unison: { voices: 1, detune_cents: 0 } });
    expect(sound.layers[0].envelope).toEqual({ attack: 0, decay: 0.1, sustain: 0, hold: 0, release: 0 });
    expect(sound.effects).toEqual({ drive: null, echo: null, reverb: null });
    expect(() => model.normalizeSound({ layers: [{ waveform: "kazoo" }] })).toThrow(/Forme d'onde/);
  });

  it("garde les matériaux et les modes à la main", () => {
    const sound = model.normalizeSound({ layers: [{ waveform: { square: { duty: 0.2 } }, pitch: { start: 300 }, envelope: { attack: 0, decay: 0.1 }, resonator: { material: { custom: { modes: [{ ratio: 1, decay: 0.3, gain: 1 }] } } } }] });
    expect(sound.layers[0].waveform).toEqual({ square: { duty: 0.2 } });
    expect(sound.layers[0].resonator).toEqual({ material: { custom: { modes: [{ ratio: 1, decay: 0.3, gain: 1 }] } }, decay: 1, brightness: 0.5, dry: 0 });
  });

  it("compare deux sons aux arrondis du moteur près", () => {
    const a = model.newSound();
    const b = model.clone(a);
    b.layers[0].envelope.decay = 0.25000001;
    expect(model.sameSound(a, b)).toBe(true);
    b.layers[0].envelope.decay = 0.26;
    expect(model.sameSound(a, b)).toBe(false);
  });

  it("nomme les notes à la française et valide les noms de fichier", () => {
    expect(model.noteOf(440)).toBe("la3");
    expect(model.noteOf(587.33)).toBe("ré4");
    expect(model.validName("sfx_eku-saut")).toBe(true);
    expect(model.validName("../x")).toBe(false);
  });
});

describe("pont avec Claude", () => {
  it("une retouche de Claude entre dans l'historique à son nom et s'annule avec ⌘Z", async () => {
    const { editor, rpc, notify } = setup();
    const sound = model.newSound("claude");
    sound.layers[0].pitch.start = 880;
    const result = (await rpc.edit_sound({ sound, label: "Plus aigu" })) as { changed: boolean; overview_png: string };
    expect(result.changed).toBe(true);
    expect(result.overview_png).toBe("PNG");
    expect(editor.doc.steps.at(-1)).toMatchObject({ label: "Plus aigu", author: "claude" });
    expect(notify).toHaveBeenCalledWith("Plus aigu");
    editor.undo();
    expect(editor.doc.take.sound.layers[0].pitch.start).toBe(440);
  });

  it("Claude ne jette jamais un son modifié : ouvrir un autre son est refusé sans force", async () => {
    const { editor, commands, rpc } = setup();
    await commands.saveAs("a");
    disk.set("b", { sound: model.newSound("b"), variants: 3, amount: 0.4, seed: 2, preset: null });
    editor.edit("Graine", (t) => (t.seed = 5));
    await expect(rpc.open_sound({ filename: "b" })).rejects.toThrow(/non enregistrées/);
    expect(editor.doc.name).toBe("a");
    await rpc.open_sound({ filename: "b_02.wav", force: true });
    expect(editor.doc.name).toBe("b");
    expect(editor.doc.take.variants).toBe(3);
  });

  it("enregistre comme le serveur MCP, puis le son n'est plus modifié", async () => {
    const { editor, rpc } = setup();
    await expect(rpc.save_sound({})).rejects.toThrow(/filename/);
    const saved = (await rpc.save_sound({ filename: "neuf" })) as { files: string[] };
    expect(saved.files).toEqual(["neuf.wav"]);
    expect(editor.doc.name).toBe("neuf");
    expect(editor.doc.dirty).toBe(false);
    disk.set("autre", { sound: model.newSound(), variants: 1, amount: 0.4, seed: 1, preset: null });
    await expect(rpc.save_sound({ filename: "autre" })).rejects.toThrow(/existe déjà/);
  });

  it("un rendu de Claude sur le son ouvert devient une étape annulable", async () => {
    const { editor, commands, rpc } = setup();
    await commands.saveAs("pas");
    const louder = model.clone(editor.doc.take);
    louder.sound.peak_db = -6;
    disk.set("pas", { ...louder, preset: null });
    const result = (await rpc.show({ filename: "pas_01" })) as { opened: boolean };
    expect(result.opened).toBe(true);
    expect(editor.doc.take.sound.peak_db).toBe(-6);
    expect(editor.doc.steps.at(-1)?.author).toBe("claude");
    expect(editor.doc.dirty).toBe(false);
    editor.undo();
    expect(editor.doc.take.sound.peak_db).toBe(-3);
  });
});
