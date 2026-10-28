/**
 * Les couches du son, une carte chacune. Chaque couche a sa couleur (la même que sa barre dans l'aperçu)
 * et un rôle : attaque, corps, queue, souffle, voix. Le rôle est donné, ou deviné d'après la forme.
 *
 * Mode simple : la carte montre ce qui se comprend d'un coup d'œil (rôle expliqué, volume, enveloppe et
 * hauteur à dessiner) ; « Tous les réglages » ouvre le reste. Mode détaillé : tout est ouvert.
 * S isole la couche, M la coupe : une aide d'écoute, jamais enregistrée.
 */

import type { Editor } from "../editor";
import {
  FILTER_MODES,
  type Layer,
  MATERIALS,
  MATERIAL_MODES,
  MAX_LAYERS,
  ROOT_HZ,
  type Take,
  WAVEFORMS,
  type BasicMaterial,
  type BasicWaveform,
  defaultFilter,
  defaultResonator,
  defaultVibrato,
  describeLayer,
  envelopeDuration,
  formatHz,
  hzOfSemitones,
  isNoise,
  materialKind,
  newLayer,
  noteOf,
  sampleOf,
  semitonesOf,
  waveformKind,
} from "../model";
import * as native from "../native";
import { type FieldSpec, type Phase, block, el, formatNumber, numberField, segmented, selectField } from "./controls";
import { EnvelopeEditor, SweepEditor } from "./curves";
import { BLOCK_HELP, ROLES, roleOf, roleText } from "./help";
import { esc, icon } from "./icons";

export const LAYER_COLORS = ["#ff9a4a", "#5ec8f2", "#b48cff", "#6fd39a", "#f2c94c", "#ff6f91", "#4fd6c6", "#c8a27a"];
export const layerColor = (i: number) => LAYER_COLORS[i % LAYER_COLORS.length];

const collapsed = new Set<number>();
/** Cartes ouvertes en entier alors que le mode simple est actif. */
const expanded = new Set<number>();

export function renderLayers(root: HTMLElement, editor: Editor): void {
  // La liste défile dans son parent (avec les panneaux d'aide) : on garde la position de lecture.
  const scroller = root.parentElement ?? root;
  const scroll = scroller.scrollTop;
  root.innerHTML = "";
  const layers = editor.doc.take.sound.layers;
  layers.forEach((layer, i) => root.append(layerCard(editor, layer, i, layers.length)));
  const actions = el("div", "layer-actions");
  const add = el("button", "add-layer", `${icon("plus", 15)} Ajouter une couche`);
  add.disabled = layers.length >= MAX_LAYERS;
  add.title = "⇧⌘N";
  add.addEventListener("click", () => addLayer(editor));
  actions.append(add);
  root.append(actions);
  scroller.scrollTop = scroll;
}

export function addLayer(editor: Editor, from?: number): void {
  const layers = editor.doc.take.sound.layers;
  if (layers.length >= MAX_LAYERS) return;
  const source = from === undefined ? null : layers[from];
  editor.edit(source ? `Couche ${from! + 1} dupliquée` : "Couche ajoutée", (take) => {
    const layer: Layer = source ? JSON.parse(JSON.stringify(source)) : { ...newLayer(), gain: 0.7 };
    take.sound.layers.splice(from === undefined ? take.sound.layers.length : from + 1, 0, layer);
  });
  if (from !== undefined) editor.remapLayers((i) => (i > from ? i + 1 : i));
}

export function deleteLayer(editor: Editor, index: number): void {
  if (editor.doc.take.sound.layers.length <= 1) return;
  editor.remapLayers((i) => (i === index ? null : i > index ? i - 1 : i));
  collapsed.clear();
  expanded.clear();
  editor.edit(`Couche ${index + 1} supprimée`, (take) => void take.sound.layers.splice(index, 1));
}

function moveLayer(editor: Editor, index: number, to: number): void {
  editor.remapLayers((i) => (i === index ? to : i === to ? index : i));
  editor.edit(`Couche ${index + 1} déplacée`, (take) => {
    const [layer] = take.sound.layers.splice(index, 1);
    take.sound.layers.splice(to, 0, layer);
  });
}

type FieldMaker = (label: string, spec: Omit<FieldSpec, "label" | "onChange">, set: (layer: Layer, v: number) => void) => HTMLElement;

function layerCard(editor: Editor, layer: Layer, i: number, count: number): HTMLElement {
  const full = !editor.simple || expanded.has(i);
  const card = el("article", `layer-card${collapsed.has(i) ? " collapsed" : ""}${editor.audible(i) ? "" : " silent"}${full ? "" : " compact"}`);
  const color = layerColor(i);
  card.style.setProperty("--layer", color);
  const at = (take: Take) => take.sound.layers[i];
  const current = () => editor.doc.take.sound.layers[i];
  const { role, guessed } = roleOf(layer);
  const name = `${role}`;
  const curves: { draw(): void }[] = [];

  // Après chaque mouvement : le titre, les dessins suivent sans tout redessiner.
  const refresh = () => {
    if (!current()) return;
    card.querySelector(".layer-desc")!.textContent = describeLayer(current());
    curves.forEach((c) => c.draw());
  };
  const field: FieldMaker = (label, spec, set) =>
    numberField({
      ...spec,
      label,
      onChange: (v: number, phase: Phase) => {
        editor.gesture(`${name} · ${label.toLowerCase()}`, phase, (take) => set(at(take), v));
        refresh();
      },
    });
  const edit = (label: string, change: (layer: Layer) => void) => editor.edit(`${name} · ${label}`, (take) => change(at(take)));

  const head = el("header", "layer-head");
  head.innerHTML = `
    <button class="icon-button collapse" title="${collapsed.has(i) ? "Déplier" : "Replier"}">${icon(collapsed.has(i) ? "chevronRight" : "chevronDown", 15)}</button>
    <span class="swatch"></span>
    <select class="role" title="Rôle de la couche dans le son">
      <option value="">${esc(guessed ? `${role} ?` : role)}</option>
      ${ROLES.map(([r]) => `<option value="${esc(r)}">${esc(r)}</option>`).join("")}
      ${guessed ? "" : '<option value="-">(sans rôle)</option>'}
    </select>
    <span class="layer-desc">${esc(describeLayer(layer))}</span>
    <span class="spacer"></span>
    <button class="chip solo${editor.solo.has(i) ? " on" : ""}" title="Écouter cette couche seule">S</button>
    <button class="chip mute${editor.muted.has(i) ? " on" : ""}" title="Couper cette couche">M</button>
    <button class="icon-button up" title="Monter" ${i === 0 ? "disabled" : ""}>${icon("arrowUp", 14)}</button>
    <button class="icon-button down" title="Descendre" ${i === count - 1 ? "disabled" : ""}>${icon("arrowDown", 14)}</button>
    <button class="icon-button dup" title="Dupliquer (⌘D)" ${count >= MAX_LAYERS ? "disabled" : ""}>${icon("copy", 14)}</button>
    <button class="icon-button del" title="Supprimer" ${count <= 1 ? "disabled" : ""}>${icon("trash", 14)}</button>`;
  head.querySelector<HTMLSelectElement>(".role")!.addEventListener("change", (e) => {
    const value = (e.target as HTMLSelectElement).value;
    if (value === "") return;
    editor.edit(`Couche ${i + 1} : rôle ${value === "-" ? "retiré" : value}`, (take) => (at(take).name = value === "-" ? null : value));
  });
  head.querySelector(".collapse")!.addEventListener("click", () => {
    if (!collapsed.delete(i)) collapsed.add(i);
    editor.emit("doc");
  });
  head.querySelector(".solo")!.addEventListener("click", () => editor.toggleSolo(i));
  head.querySelector(".mute")!.addEventListener("click", () => editor.toggleMute(i));
  head.querySelector(".up")!.addEventListener("click", () => moveLayer(editor, i, i - 1));
  head.querySelector(".down")!.addEventListener("click", () => moveLayer(editor, i, i + 1));
  head.querySelector(".dup")!.addEventListener("click", () => addLayer(editor, i));
  head.querySelector(".del")!.addEventListener("click", () => deleteLayer(editor, i));
  card.append(head);
  if (collapsed.has(i)) return card;

  const explain = roleText(role);
  if (explain) card.append(el("p", "role-help", esc(explain)));

  const body = el("div", "layer-body");
  const sample = sampleOf(layer.waveform);
  const noise = isNoise(layer.waveform);
  const length = () => Math.max(envelopeDuration(current().envelope), 0.05);

  // Enveloppe : on la dessine.
  const envelope = new EnvelopeEditor(
    () => current().envelope,
    (env, phase) => {
      editor.gesture(`${name} · enveloppe`, phase, (take) => (at(take).envelope = env));
      refresh();
    },
    color,
  );
  curves.push(envelope);

  // Hauteur, ou transposition d'un enregistrement : on la dessine aussi.
  const pitchMatters = !noise || !!layer.resonator;
  const pitchTitle = sample ? "Transposition" : noise && layer.resonator ? "Fondamentale de l'objet" : "Hauteur";
  const pitchAxis = sample
    ? { min: hzOfSemitones(-24), max: hzOfSemitones(24), label: (v: number) => semitoneLabel(semitonesOf(v)), grid: [-12, 0, 12].map(hzOfSemitones) }
    : { min: 30, max: 8000, label: (v: number) => `${formatHz(v)} · ${noteOf(v)}`, grid: [100, 440, 2000] };
  const pitch = pitchMatters
    ? new SweepEditor(
        () => ({ sweep: current().pitch, length: length() }),
        pitchAxis,
        (sweep, phase) => {
          editor.gesture(`${name} · ${pitchTitle.toLowerCase()}`, phase, (take) => (at(take).pitch = sweep));
          refresh();
        },
        color,
      )
    : null;
  if (pitch) curves.push(pitch);

  if (!full) {
    // Mode simple : l'essentiel, à dessiner.
    const essentials = el("div", "layer-simple");
    const left = el("div", "simple-col");
    left.append(
      el("h4", "", `Volume dans le temps <span>${esc(BLOCK_HELP.Enveloppe)}</span>`),
      envelope.canvas,
      field("Volume", { value: layer.gain, min: 0, max: 2, floor: 0, reset: 1, hint: "Volume de la couche dans le mélange." }, (l, v) => (l.gain = v)),
    );
    essentials.append(left);
    if (pitch) {
      const right = el("div", "simple-col");
      right.append(el("h4", "", `${esc(pitchTitle)} <span>${esc(BLOCK_HELP[pitchTitle])}</span>`), pitch.canvas);
      if (sample) right.append(sampleControls(editor, i, sample.reverse, edit));
      essentials.append(right);
    } else {
      const right = el("div", "simple-col");
      right.append(el("h4", "", `Couleur du bruit <span>${esc(BLOCK_HELP.Filtre)}</span>`));
      right.append(layer.filter ? filterCurve(editor, i, name, color, refresh, curves, length).canvas : el("p", "muted-note", "Pas de filtre : le bruit est complet. Ouvre tous les réglages pour le colorer."));
      essentials.append(right);
    }
    card.append(essentials);
    const more = el("button", "link more", `Tous les réglages de cette couche ${icon("chevronRight", 12)}`);
    more.addEventListener("click", () => {
      expanded.add(i);
      editor.emit("doc");
    });
    card.append(more);
    return card;
  }

  // Source
  const source: HTMLElement[] = [
    selectField("Forme", WAVEFORMS, waveformKind(layer.waveform), (value) => void chooseSource(editor, value, edit)),
  ];
  if (typeof layer.waveform === "object" && "square" in layer.waveform) {
    source.push(field("Rapport", { value: layer.waveform.square.duty, min: 0.05, max: 0.95, reset: 0.5, hint: "Rapport cyclique : 0,5 = carré parfait, plus bas = plus nasillard." }, (l, v) => {
      if (typeof l.waveform === "object" && "square" in l.waveform) l.waveform.square.duty = v;
    }));
  }
  if (sample) source.push(sampleControls(editor, i, sample.reverse, edit));
  source.push(
    field("Volume", { value: layer.gain, min: 0, max: 2, floor: 0, reset: 1, hint: "Volume relatif de la couche dans le mélange (1 = référence)." }, (l, v) => (l.gain = v)),
    field("Départ", { value: layer.start, min: 0, max: 2, scale: "sq", floor: 0, unit: "s", reset: 0, hint: "Décalage de la couche par rapport au début du son." }, (l, v) => (l.start = v)),
  );
  body.append(block("Source", source, undefined, BLOCK_HELP.Source));

  // Hauteur
  const pitchFields: HTMLElement[] = [];
  if (pitch) {
    pitchFields.push(pitch.canvas);
    if (sample) {
      pitchFields.push(
        field("Départ", { value: Number(semitonesOf(layer.pitch.start).toFixed(2)), min: -24, max: 24, unit: "dt", reset: 0, hint: "Demi-tons par rapport à la vitesse d'origine." }, (l, v) => (l.pitch.start = hzOfSemitones(v))),
        field("Arrivée", { value: Number(semitonesOf(layer.pitch.end).toFixed(2)), min: -24, max: 24, unit: "dt", reset: 0 }, (l, v) => (l.pitch.end = hzOfSemitones(v))),
      );
    } else {
      pitchFields.push(
        field("Départ", { value: layer.pitch.start, min: 20, max: 12000, scale: "log", unit: "Hz", extra: noteOf, floor: 1 }, (l, v) => (l.pitch.start = v)),
        field("Arrivée", { value: layer.pitch.end, min: 20, max: 12000, scale: "log", unit: "Hz", extra: noteOf, floor: 1 }, (l, v) => (l.pitch.end = v)),
      );
    }
    pitchFields.push(field("Glissement", { value: layer.pitch.time, min: 0, max: 2, scale: "sq", unit: "s", floor: 0, reset: 0, hint: "Durée du glissement de départ à arrivée." }, (l, v) => (l.pitch.time = v)));
  } else {
    pitchFields.push(el("p", "muted-note", "Un bruit n'a pas de hauteur : c'est le filtre qui lui donne sa couleur."));
  }
  body.append(block(pitchTitle, pitchFields, undefined, BLOCK_HELP[pitchTitle]));

  // Enveloppe
  body.append(
    block(
      "Enveloppe",
      [
        envelope.canvas,
        field("Attaque", { value: layer.envelope.attack, min: 0, max: 1, scale: "sq", unit: "s", floor: 0, hint: "Montée du silence au maximum : 0,001 = attaque sèche." }, (l, v) => (l.envelope.attack = v)),
        field("Déclin", { value: layer.envelope.decay, min: 0, max: 4, scale: "sq", unit: "s", floor: 0, hint: "Descente du maximum vers le palier." }, (l, v) => (l.envelope.decay = v)),
        field("Palier", { value: layer.envelope.sustain, min: 0, max: 1, floor: 0, ceil: 1, reset: 0, hint: "Niveau tenu après le déclin. 0 = son percussif." }, (l, v) => (l.envelope.sustain = v)),
        field("Tenue", { value: layer.envelope.hold, min: 0, max: 4, scale: "sq", unit: "s", floor: 0, reset: 0, hint: "Durée du palier." }, (l, v) => (l.envelope.hold = v)),
        field("Extinction", { value: layer.envelope.release, min: 0, max: 4, scale: "sq", unit: "s", floor: 0, reset: 0, hint: "Retour au silence depuis le palier." }, (l, v) => (l.envelope.release = v)),
      ],
      undefined,
      BLOCK_HELP.Enveloppe,
    ),
  );

  // Filtre
  const filter = layer.filter;
  body.append(
    block(
      "Filtre",
      filter
        ? [
            segmented(FILTER_MODES, filter.mode, (mode) => edit("mode du filtre", (l) => (l.filter!.mode = mode as typeof filter.mode))),
            filterCurve(editor, i, name, color, refresh, curves, length).canvas,
            field("Coupure", { value: filter.cutoff.start, min: 20, max: 20000, scale: "log", unit: "Hz", floor: 1 }, (l, v) => (l.filter!.cutoff.start = v)),
            field("Arrivée", { value: filter.cutoff.end, min: 20, max: 20000, scale: "log", unit: "Hz", floor: 1, hint: "Coupure atteinte après le balayage." }, (l, v) => (l.filter!.cutoff.end = v)),
            field("Balayage", { value: filter.cutoff.time, min: 0, max: 2, scale: "sq", unit: "s", floor: 0, reset: 0 }, (l, v) => (l.filter!.cutoff.time = v)),
            field("Résonance", { value: filter.resonance, min: 0, max: 0.95, floor: 0, ceil: 0.95, reset: 0.3, hint: "0 = doux, 0,3 = neutre, 0,95 = sifflant." }, (l, v) => (l.filter!.resonance = v)),
          ]
        : [],
      { on: !!filter, onChange: (on) => edit(on ? "filtre activé" : "filtre retiré", (l) => (l.filter = on ? defaultFilter() : null)) },
      BLOCK_HELP.Filtre,
    ),
  );

  // Résonateur
  const res = layer.resonator;
  const resBody: HTMLElement[] = [];
  if (res) {
    resBody.push(
      selectField("Matière", MATERIALS, materialKind(res.material), (value) =>
        edit("matière", (l) => {
          if (value !== "custom") l.resonator!.material = value as BasicMaterial;
          else {
            const from = materialKind(l.resonator!.material);
            const table = MATERIAL_MODES[(from in MATERIAL_MODES ? from : "wood") as BasicMaterial];
            l.resonator!.material = { custom: { modes: table.map(([ratio, decay, gain]) => ({ ratio, decay, gain })) } };
          }
        }),
      ),
      field("Durée", { value: res.decay, min: 0.1, max: 4, scale: "log", unit: "×", floor: 0.01, reset: 1, hint: "0,5 = objet étouffé, 2 = objet qui chante." }, (l, v) => (l.resonator!.decay = v)),
      field("Brillance", { value: res.brightness, min: 0, max: 1, floor: 0, ceil: 1, reset: 0.5, hint: "0 = sourd, 0,5 = la matière telle quelle, 1 = brillant." }, (l, v) => (l.resonator!.brightness = v)),
      field("Coup audible", { value: res.dry, min: 0, max: 1, floor: 0, ceil: 1, reset: 0, hint: "Part du coup entendu tel quel : 0 = l'objet seul, 1 = le coup seul." }, (l, v) => (l.resonator!.dry = v)),
    );
    if (typeof res.material === "object") resBody.push(modesTable(editor, i, res.material.custom.modes.length, field));
  }
  body.append(
    block(
      "Résonateur",
      resBody,
      { on: !!res, onChange: (on) => edit(on ? "résonateur activé" : "résonateur retiré", (l) => (l.resonator = on ? defaultResonator() : null)) },
      BLOCK_HELP.Résonateur,
    ),
  );

  // Modulation
  const vib = layer.vibrato;
  body.append(
    block(
      "Unisson",
      [
        field("Voix", { value: layer.unison.voices, min: 1, max: 16, integer: true, floor: 1, ceil: 16, reset: 1, hint: "Oscillateurs désaccordés empilés : un son plus large." }, (l, v) => (l.unison.voices = v)),
        field("Désaccord", { value: layer.unison.detune_cents, min: 0, max: 100, unit: "cents", floor: 0, reset: 0, hint: "Écart entre les voix extrêmes (10 à 25 pour un « supersaw »)." }, (l, v) => (l.unison.detune_cents = v)),
      ],
      undefined,
      BLOCK_HELP.Unisson,
    ),
    block(
      "Vibrato",
      vib
        ? [
            field("Vitesse", { value: vib.rate_hz, min: 0.1, max: 30, scale: "log", unit: "Hz", floor: 0, reset: 6 }, (l, v) => (l.vibrato!.rate_hz = v)),
            field("Profondeur", { value: vib.depth_semitones, min: 0, max: 4, scale: "sq", unit: "dt", floor: 0, reset: 0.3, hint: "Amplitude en demi-tons." }, (l, v) => (l.vibrato!.depth_semitones = v)),
          ]
        : [],
      { on: !!vib, onChange: (on) => edit(on ? "vibrato activé" : "vibrato retiré", (l) => (l.vibrato = on ? defaultVibrato() : null)) },
      BLOCK_HELP.Vibrato,
    ),
  );
  card.append(body);
  if (editor.simple) {
    const less = el("button", "link more", `Moins de réglages ${icon("chevronDown", 12)}`);
    less.addEventListener("click", () => {
      expanded.delete(i);
      editor.emit("doc");
    });
    card.append(less);
  }
  return card;
}

function semitoneLabel(st: number): string {
  const rounded = Math.round(st * 10) / 10;
  return rounded === 0 ? "vitesse d'origine" : `${rounded > 0 ? "+" : "−"}${formatNumber(Math.abs(rounded))} dt`;
}

function filterCurve(editor: Editor, i: number, name: string, color: string, refresh: () => void, curves: { draw(): void }[], length: () => number): SweepEditor {
  const curve = new SweepEditor(
    () => ({ sweep: editor.doc.take.sound.layers[i].filter?.cutoff ?? { start: 1000, end: 1000, time: 0 }, length: length() }),
    { min: 20, max: 20000, label: formatHz, grid: [100, 1000, 10000] },
    (sweep, phase) => {
      editor.gesture(`${name} · coupure du filtre`, phase, (take) => {
        const f = take.sound.layers[i].filter;
        if (f) f.cutoff = sweep;
      });
      refresh();
    },
    color,
  );
  curves.push(curve);
  return curve;
}

/** Choix de l'enregistrement, lecture à l'envers, et durée de la couche calée sur l'enregistrement. */
function sampleControls(editor: Editor, i: number, reverse: boolean, edit: (label: string, change: (layer: Layer) => void) => void): HTMLElement {
  const root = el("div", "sample-controls");
  const recordings = editor.library?.recordings ?? [];
  const current = sampleOf(editor.doc.take.sound.layers[i].waveform)!;
  const options: [string, string][] = recordings.map(([name, seconds]) => [name, `${name.replace(/^voix\//, "")} (${formatNumber(seconds)} s)`]);
  if (!options.some(([name]) => name === current.file)) options.unshift([current.file, `${current.file} (introuvable)`]);
  root.append(
    selectField("Prise", options, current.file, (file) =>
      edit("enregistrement", (l) => {
        const s = sampleOf(l.waveform);
        if (s) s.file = file;
      }),
    ),
    segmented([["normal", "Normal"], ["reverse", "À l'envers"]], reverse ? "reverse" : "normal", (value) =>
      edit(value === "reverse" ? "lecture à l'envers" : "lecture à l'endroit", (l) => {
        const s = sampleOf(l.waveform);
        if (s) s.reverse = value === "reverse";
      }),
    ),
  );
  const fit = el("button", "link", "Caler la durée sur la prise");
  fit.title = "Règle la tenue de l'enveloppe pour entendre l'enregistrement en entier, à sa transposition.";
  fit.addEventListener("click", () => {
    const seconds = recordings.find(([name]) => name === current.file)?.[1];
    if (!seconds) return;
    edit("durée calée sur la prise", (l) => {
      const speed = Math.max(l.pitch.start, l.pitch.end) / ROOT_HZ;
      const e = l.envelope;
      e.sustain = e.sustain || 1;
      e.hold = Number(Math.max(0, seconds / speed - e.attack - e.decay).toFixed(3));
    });
  });
  root.append(fit);
  return root;
}

/** Changer la source d'une couche. Passer à un enregistrement demande d'en avoir un. */
async function chooseSource(editor: Editor, value: string, edit: (label: string, change: (layer: Layer) => void) => void): Promise<void> {
  if (value === "sample") {
    const recordings = editor.library?.recordings ?? [];
    if (!recordings.length) {
      await native.alertError("Aucun enregistrement pour l'instant : utilise « Enregistrer ma voix » (bouton micro, en haut).");
      editor.emit("doc");
      return;
    }
    const [file, seconds] = recordings[recordings.length - 1];
    edit("source : enregistrement", (l) => {
      l.waveform = { sample: { file, reverse: false } };
      l.pitch = { start: ROOT_HZ, end: ROOT_HZ, time: 0 };
      l.envelope = { attack: 0.005, decay: 0, sustain: 1, hold: Number(seconds.toFixed(3)), release: 0.05 };
    });
    return;
  }
  edit("forme d'onde", (l) => {
    const wasSample = !!sampleOf(l.waveform);
    l.waveform = value === "square" ? { square: { duty: 0.5 } } : (value as BasicWaveform);
    if (wasSample) l.envelope = { attack: 0.005, decay: 0.25, sustain: 0, hold: 0, release: 0 };
  });
}

function modesTable(editor: Editor, layerIndex: number, count: number, field: FieldMaker): HTMLElement {
  const root = el("div", "modes");
  const material = editor.doc.take.sound.layers[layerIndex].resonator?.material;
  const modes = material && typeof material === "object" ? material.custom.modes : [];
  modes.forEach((mode, k) => {
    const row = el("div", "mode-row");
    row.append(
      el("span", "mode-index", String(k + 1)),
      field("Rapport", { value: mode.ratio, min: 0.25, max: 16, scale: "log", unit: "×", floor: 0.01 }, (l, v) => void (modeOf(l, k) && (modeOf(l, k)!.ratio = v))),
      field("Durée", { value: mode.decay, min: 0.01, max: 8, scale: "log", unit: "s", floor: 0.001 }, (l, v) => void (modeOf(l, k) && (modeOf(l, k)!.decay = v))),
      field("Niveau", { value: mode.gain, min: 0, max: 1, floor: 0 }, (l, v) => void (modeOf(l, k) && (modeOf(l, k)!.gain = v))),
    );
    const remove = el("button", "icon-button", icon("minus", 13));
    remove.title = "Retirer ce mode";
    remove.disabled = count <= 1;
    remove.addEventListener("click", () =>
      editor.edit(`Couche ${layerIndex + 1} · mode retiré`, (take) => {
        const m = take.sound.layers[layerIndex].resonator?.material;
        if (m && typeof m === "object") m.custom.modes.splice(k, 1);
      }),
    );
    row.append(remove);
    root.append(row);
  });
  const add = el("button", "button small", `${icon("plus", 13)} Mode`);
  add.disabled = count >= 16;
  add.addEventListener("click", () =>
    editor.edit(`Couche ${layerIndex + 1} · mode ajouté`, (take) => {
      const m = take.sound.layers[layerIndex].resonator?.material;
      if (m && typeof m === "object") {
        const last = m.custom.modes[m.custom.modes.length - 1];
        m.custom.modes.push({ ratio: Number(((last?.ratio ?? 1) * 1.5).toFixed(2)), decay: last?.decay ?? 0.3, gain: 0.3 });
      }
    }),
  );
  root.append(add);
  return root;
}

function modeOf(layer: Layer, k: number) {
  const m = layer.resonator?.material;
  return m && typeof m === "object" ? m.custom.modes[k] : undefined;
}
