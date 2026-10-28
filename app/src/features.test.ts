import { describe, expect, it } from "vitest";
import { SoundDoc } from "./doc";
import * as model from "./model";
import { guessRole, roleOf } from "./ui/help";
import { MACROS } from "./ui/macros";
import { cut, encodeWav, trimBounds } from "./ui/recorder";
import { mutate, rng } from "./ui/variations";

const macro = (id: string) => MACROS.find((m) => m.id === id)!;

function take(): model.Take {
  const t = model.newTake();
  t.sound.layers.push({ ...model.newLayer(), waveform: "white_noise", filter: model.defaultFilter(), envelope: { attack: 0, decay: 0.03, sustain: 0, hold: 0, release: 0 } });
  t.sound.effects.reverb = { room: 0.4, damping: 0.5, mix: 0.2 };
  return t;
}

describe("réglages en mots", () => {
  it("plus aigu d'une octave : toutes les hauteurs doublent, les filtres suivent à moitié", () => {
    const base = take();
    const out = macro("pitch").apply(base, 1);
    expect(out.sound.layers[0].pitch.start).toBeCloseTo(880, 0);
    expect(out.sound.layers[1].filter!.cutoff.start).toBeCloseTo(2000 * Math.SQRT2, -1);
    expect(base.sound.layers[0].pitch.start).toBe(440); // la base n'est jamais modifiée
  });

  it("plus long étire les durées ; plus sec retire l'espace jusqu'à l'enlever", () => {
    const longer = macro("length").apply(take(), 1);
    expect(longer.sound.layers[0].envelope.decay).toBeCloseTo(0.25 * Math.pow(2, 1.5), 3);
    expect(macro("space").apply(take(), -1).sound.effects.reverb).toBeNull();
    const far = macro("space").apply({ ...take(), sound: { ...take().sound, effects: { drive: null, echo: null, reverb: null } } }, 0.5);
    expect(far.sound.effects.reverb!.mix).toBeGreaterThan(0.1);
  });

  it("plus agressif ajoute de la saturation, plus doux la retire", () => {
    const harsh = macro("bite").apply(take(), 1);
    expect(harsh.sound.effects.drive).toBeGreaterThan(2);
    expect(macro("bite").apply(harsh, -1).sound.effects.drive).toBeNull();
  });

  it("plus soufflé monte les bruits et baisse les notes", () => {
    const out = macro("texture").apply(take(), 1);
    expect(out.sound.layers[1].gain).toBeGreaterThan(1);
    expect(out.sound.layers[0].gain).toBeLessThan(1);
  });
});

describe("variations", () => {
  it("même graine, mêmes propositions ; audace nulle, son inchangé", () => {
    const base = take();
    expect(mutate(base, 0.5, rng(3))).toEqual(mutate(base, 0.5, rng(3)));
    expect(mutate(base, 0.5, rng(3))).not.toEqual(mutate(base, 0.5, rng(4)));
    expect(model.sameSound(mutate(base, 0, rng(9)).sound, base.sound)).toBe(true);
  });

  it("reste dans les limites du moteur, même à audace maximale", () => {
    const random = rng(1);
    for (let k = 0; k < 200; k++) {
      const t = mutate(take(), 1, random);
      for (const l of t.sound.layers) {
        expect(l.envelope.sustain).toBeGreaterThanOrEqual(0);
        expect(l.envelope.sustain).toBeLessThanOrEqual(1);
        if (l.filter) expect(l.filter.resonance).toBeLessThanOrEqual(0.95);
        expect(l.gain).toBeGreaterThan(0);
      }
    }
  });
});

describe("enregistrement", () => {
  it("retire le silence autour de la prise, avec une petite marge", () => {
    const rate = 1000;
    const data = new Float32Array(1000);
    for (let i = 400; i < 600; i++) data[i] = Math.sin(i);
    const [from, to] = trimBounds(data, rate);
    expect(from).toBeGreaterThan(350);
    expect(from).toBeLessThan(401);
    expect(to).toBeGreaterThan(599);
    expect(to).toBeLessThan(650);
    const piece = cut(data, from, to, rate);
    expect(piece.length).toBe(to - from);
    expect(piece[0]).toBe(0);
  });

  it("écrit un WAV 16 bits mono que le moteur sait lire", () => {
    const bytes = encodeWav(new Float32Array([0, 1, -1]), 48000);
    const view = new DataView(bytes.buffer);
    expect(String.fromCharCode(...bytes.subarray(0, 4))).toBe("RIFF");
    expect(view.getUint32(24, true)).toBe(48000);
    expect(view.getUint16(22, true)).toBe(1);
    expect(view.getInt16(46, true)).toBe(32767);
    expect(bytes.length).toBe(44 + 6);
  });

  it("une couche d'enregistrement se relit, se décrit et a le rôle Voix", () => {
    const layer = model.sampleLayer("voix/souffle", 0.8);
    const sound = model.normalizeSound({ layers: [layer] });
    expect(sound.layers[0].waveform).toEqual({ sample: { file: "voix/souffle", reverse: false } });
    expect(sound.layers[0].envelope.hold).toBe(0.8);
    sound.layers[0].pitch.start = model.hzOfSemitones(-12);
    expect(model.describeLayer(sound.layers[0])).toBe("Enregistrement « souffle » · -12 demi-tons");
    expect(roleOf(sound.layers[0])).toEqual({ role: "Voix", guessed: false });
  });
});

describe("rôles et historique", () => {
  it("devine le rôle d'après la forme", () => {
    expect(guessRole({ ...model.newLayer(), envelope: { attack: 0, decay: 0.03, sustain: 0, hold: 0, release: 0 } })).toBe("Attaque");
    expect(guessRole(model.newLayer())).toBe("Corps");
    expect(guessRole({ ...model.newLayer(), start: 0.1 })).toBe("Queue");
    expect(guessRole({ ...model.newLayer(), waveform: "white_noise" })).toBe("Souffle");
    expect(roleOf({ ...model.newLayer(), name: "Corps" })).toEqual({ role: "Corps", guessed: false });
  });

  it("un geste se renomme à sa fin ; la référence « avant » est la version enregistrée", () => {
    const doc = new SoundDoc(model.newTake(), "son", null);
    doc.begin("Plus aigu");
    doc.take.sound.layers[0].pitch.start = 880;
    doc.relabel("Plus aigu (+12 demi-tons)");
    doc.commit();
    expect(doc.steps.at(-1)!.label).toBe("Plus aigu (+12 demi-tons)");
    expect(doc.reference.sound.layers[0].pitch.start).toBe(440);
  });
});
