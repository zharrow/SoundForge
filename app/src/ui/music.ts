/**
 * Les panneaux de l'onglet Musique.
 * - Barre du haut : lecture, scénarios de jeu (exploration, danger, combat), stems à couper.
 * - Gauche : les morceaux du dossier musique/.
 * - Centre : le morceau (tempo, mesures, grille, zoom), la vue d'ensemble des pistes, le piano roll.
 * - Droite : la piste choisie (instrument, stem, volume, placement, espace, transposition, jeu humain), la note
 *   choisie, le morceau (espace commun, niveau, graine), l'enregistrement et l'historique.
 */

import type { MusicEditor } from "../music-editor";
import { type InstrumentInfo, STEM_NAMES, detachPatterns, frenchName, instrumentLabel, midiOf, newTrack, noteName, outOfRange, patternNotes, stemOf } from "../music-model";
import * as native from "../native";
import { type FieldSpec, type Phase, block, el, formatNumber, numberField, segmented } from "./controls";
import { esc, icon } from "./icons";
import { layerColor } from "./layers";

export interface MusicCommands {
  [command: string]: ((...args: never[]) => unknown) | undefined;
  openPiece(name: string, ask?: boolean): Promise<boolean>;
  newPiece(): Promise<void>;
  save(): Promise<void>;
  save_as(): Promise<void>;
}

/** Scénarios de jeu : quels stems entendre selon la situation (direction audio d'Eku). */
const SCENES: [string, string, string[]][] = [
  ["Exploration", "Ce qu'on entend en explorant", ["ambience", "melody"]],
  ["Danger", "Un danger approche", ["ambience", "melody", "tension"]],
  ["Combat", "En plein combat", ["ambience", "tension", "combat"]],
  ["Tout", "Tous les stems", []],
];

// ---------- Barre du haut ----------

export function renderMusicTopbar(root: HTMLElement, editor: MusicEditor): void {
  const { doc, player, rendered } = editor;
  const levels = new Map(rendered?.stems ?? []);
  root.innerHTML = `
    <div class="doc-title">
      ${icon("wave", 16)}
      <strong>${esc(doc.name ?? "Morceau sans nom")}</strong>
      ${doc.dirty ? '<span class="dirty" title="Modifications non enregistrées">●</span>' : ""}
    </div>
    <div class="transport">
      <button class="play-button${player.playing ? " on" : ""}" data-music="play" title="Écouter / arrêter (Espace)">${icon(player.playing ? "stop" : "play", 16)}</button>
      <span class="render-state">${editor.rendering ? "rendu…" : editor.renderError ? "⚠" : ""}</span>
    </div>
    <div class="scenes">${SCENES.map(([name, hint]) => `<button class="chip" data-scene="${name}" title="${esc(hint)}">${name}</button>`).join("")}</div>
    <div class="stem-toggles">${editor.stems
      .map((stem) => {
        const on = player.on.get(stem) !== false;
        return `<button class="stem-toggle${on ? " on" : ""}" data-stem="${esc(stem)}" title="${on ? "Couper" : "Faire entendre"} ce stem (${levels.has(stem) ? `${formatNumber(levels.get(stem)!)} dB` : "—"})">${esc(STEM_NAMES[stem] ?? stem)}</button>`;
      })
      .join("")}</div>
    <span class="spacer"></span>`;
  root.onclick = (e) => {
    const target = e.target as HTMLElement;
    const stem = target.closest<HTMLElement>("[data-stem]")?.dataset.stem;
    const scene = target.closest<HTMLElement>("[data-scene]")?.dataset.scene;
    if (target.closest("[data-music=play]")) player.toggle();
    else if (stem) editor.setStemOn(stem, player.on.get(stem) === false);
    else if (scene) {
      const wanted = SCENES.find(([name]) => name === scene)![2];
      for (const s of editor.stems) editor.setStemOn(s, !wanted.length || wanted.includes(s));
    }
  };
}

// ---------- Bibliothèque ----------

export function renderMusicLibrary(root: HTMLElement, editor: MusicEditor, commands: MusicCommands): void {
  const lib = editor.library;
  root.innerHTML = `
    <div class="library-head">
      <h2>Morceaux</h2>
      <button class="icon-button" data-cmd="newPiece" title="Nouveau morceau">${icon("plus", 16)}</button>
    </div>
    <div class="library-list">
      ${(lib?.pieces ?? []).map((p) => `
        <button class="lib-item${p.name === editor.doc.name ? " current" : ""}" data-piece="${esc(p.name)}">
          <span class="lib-name">${esc(p.name)}</span>
          <span class="lib-meta">${p.bars} mes.</span>
        </button>`).join("") || `<p class="muted-note">${lib ? "Aucun morceau pour l'instant." : "Chargement…"}</p>`}
    </div>
    ${lib?.catalogue_error ? `<p class="muted-note warn">${esc(lib.catalogue_error)}</p>` : ""}
    <p class="library-note">Les stems et la recette s'écrivent dans <code>musique/</code>. Claude les lit et les écrit aussi.</p>`;
  root.onclick = (e) => {
    const target = e.target as HTMLElement;
    const piece = target.closest<HTMLElement>("[data-piece]")?.dataset.piece;
    if (piece) void commands.openPiece(piece);
    else if (target.closest("[data-cmd=newPiece]")) void commands.newPiece();
  };
}

// ---------- Centre : morceau, vue d'ensemble ----------

export function renderPieceBar(root: HTMLElement, editor: MusicEditor): void {
  const m = editor.music;
  root.innerHTML = "";
  const field = (label: string, spec: Omit<FieldSpec, "label" | "onChange">, history: string, set: (v: number) => void) =>
    numberField({ ...spec, label, onChange: (v: number, phase: Phase) => editor.gesture(history, phase, () => set(v)) });
  root.append(
    field("Tempo", { value: m.bpm, min: 40, max: 200, integer: true, unit: "BPM", floor: 20, ceil: 300, hint: "Battements par minute." }, "Tempo", (v) => (editor.music.bpm = v)),
    field("Mesures", { value: m.bars, min: 1, max: 64, integer: true, floor: 1, ceil: 128, hint: "Longueur de la boucle." }, "Nombre de mesures", (v) => (editor.music.bars = v)),
    field("Temps", { value: m.beats_per_bar, min: 2, max: 7, integer: true, floor: 1, ceil: 12, hint: "Temps par mesure : 4 pour du 4/4, 3 pour une valse." }, "Temps par mesure", (v) => (editor.music.beats_per_bar = v)),
  );
  const tools = el("div", "piece-tools");
  tools.append(
    el("span", "muted", "Grille"),
    segmented([["0.25", "¼"], ["0.5", "½"], ["1", "1"]], String(editor.snap), (v) => {
      editor.snap = Number(v);
      editor.emit("doc");
    }),
  );
  const zoomOut = el("button", "icon-button", icon("minus", 14));
  zoomOut.title = "Dézoomer";
  zoomOut.onclick = () => {
    editor.zoom = Math.max(8, editor.zoom / 1.4);
    editor.emit("doc");
  };
  const zoomIn = el("button", "icon-button", icon("plus", 14));
  zoomIn.title = "Zoomer";
  zoomIn.onclick = () => {
    editor.zoom = Math.min(160, editor.zoom * 1.4);
    editor.emit("doc");
  };
  const others = el("button", `chip${editor.showOthers ? " on" : ""}`, "Autres pistes");
  others.title = "Voir les notes des autres pistes en filigrane";
  others.onclick = () => {
    editor.showOthers = !editor.showOthers;
    editor.emit("doc");
  };
  tools.append(zoomOut, zoomIn, others);
  root.append(tools);
}

export function renderTracks(root: HTMLElement, editor: MusicEditor): void {
  const m = editor.music;
  root.innerHTML = "";
  const beats = m.bars * m.beats_per_bar;
  m.tracks.forEach((track, i) => {
    const row = el("div", `track-row${i === editor.selected ? " current" : ""}${editor.audible(i) ? "" : " silent"}`);
    row.style.setProperty("--layer", layerColor(i));
    const warnings = outOfRange(track, editor.instruments);
    row.innerHTML = `
      <div class="track-head">
        <span class="swatch"></span>
        <strong>${esc(track.name)}</strong>
        <span class="track-stem">${esc(STEM_NAMES[stemOf(track)] ?? stemOf(track))}</span>
        ${warnings.length ? `<span class="warn" title="${warnings.length} note(s) hors de la tessiture de l'instrument">⚠</span>` : ""}
        <span class="spacer"></span>
        <button class="chip solo${editor.solo.has(i) ? " on" : ""}" title="Écouter cette piste seule">S</button>
        <button class="chip mute${editor.muted.has(i) ? " on" : ""}" title="Couper cette piste">M</button>
      </div>
      <canvas class="track-lane"></canvas>`;
    row.querySelector(".solo")!.addEventListener("click", (e) => (e.stopPropagation(), editor.toggleSolo(i)));
    row.querySelector(".mute")!.addEventListener("click", (e) => (e.stopPropagation(), editor.toggleMute(i)));
    row.addEventListener("click", () => editor.select(i));
    const canvas = row.querySelector("canvas")!;
    requestAnimationFrame(() => drawLane(canvas, editor, i, beats));
    root.append(row);
  });
  const add = el("button", "add-layer", `${icon("plus", 15)} Ajouter une piste`);
  add.addEventListener("click", () => {
    const name = uniqueName(m.tracks.map((t) => t.name), "piste");
    editor.edit("Piste ajoutée", (take) => void take.music.tracks.push(newTrack(name, editor.instruments[0]?.id ?? "piano", "melody")));
    editor.select(m.tracks.length - 1);
  });
  root.append(add);
}

/** La piste en miniature : ses notes sur toute la longueur du morceau. */
function drawLane(canvas: HTMLCanvasElement, editor: MusicEditor, index: number, beats: number): void {
  const ratio = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = Math.round(w * ratio);
  canvas.height = Math.round(h * ratio);
  const g = canvas.getContext("2d")!;
  g.setTransform(ratio, 0, 0, ratio, 0, 0);
  const m = editor.music;
  const track = m.tracks[index];
  g.fillStyle = "rgba(255,255,255,0.06)";
  for (let bar = 0; bar <= m.bars; bar++) g.fillRect(Math.round((bar * m.beats_per_bar * w) / beats), 0, 1, h);
  const notes = [...track.notes.map((n) => ({ ...n, midi: midiOf(n.note) })), ...patternNotes(m, track)];
  if (!notes.length) return;
  const lo = Math.min(...notes.map((n) => n.midi));
  const hi = Math.max(...notes.map((n) => n.midi));
  g.fillStyle = layerColor(index);
  for (const n of notes) {
    const y = hi === lo ? h / 2 - 1 : 2 + ((hi - n.midi) / (hi - lo)) * (h - 6);
    g.globalAlpha = 0.4 + 0.6 * n.vel;
    g.fillRect((n.at * w) / beats, y, Math.max(1.5, (n.len * w) / beats - 1), 2.5);
  }
  g.globalAlpha = 1;
}

// ---------- Droite : piste, note, morceau ----------

export function renderMusicInspector(root: HTMLElement, editor: MusicEditor, commands: MusicCommands): void {
  const scroll = root.scrollTop;
  root.innerHTML = "";
  const { doc } = editor;
  const track = editor.track;
  const field = (label: string, spec: Omit<FieldSpec, "label" | "onChange">, history: string, set: (v: number) => void) =>
    numberField({ ...spec, label, onChange: (v: number, phase: Phase) => editor.gesture(history, phase, () => set(v)) });

  const file = el("section", "file-card");
  file.innerHTML = `
    <div class="file-name">${doc.name ? esc(doc.name) : '<span class="muted">jamais enregistré</span>'}${doc.dirty ? ' <span class="dirty">●</span>' : ""}</div>
    <div class="file-actions">
      <button class="button primary" data-cmd="save" title="⌘S">${icon("save", 14)} Enregistrer</button>
      <button class="button" data-cmd="save_as" title="⇧⌘S">Enregistrer sous…</button>
    </div>
    ${editor.renderError ? `<p class="render-error">${esc(editor.renderError)}</p>` : ""}`;
  file.onclick = (e) => {
    const cmd = (e.target as HTMLElement).closest<HTMLElement>("[data-cmd]")?.dataset.cmd;
    if (cmd) void commands[cmd]?.();
  };
  root.append(file);

  if (track) {
    const t = editor.selected;
    const set = (label: string, change: () => void) => editor.edit(`${track.name} · ${label}`, change);
    const name = el("input", "track-name") as HTMLInputElement;
    name.value = track.name;
    name.spellcheck = false;
    name.addEventListener("change", () => {
      const value = uniqueName(editor.music.tracks.filter((_, k) => k !== t).map((x) => x.name), name.value.trim().replace(/\s+/g, "_") || "piste");
      set("renommée", () => (editor.music.tracks[t].name = value));
    });
    const stems = [...new Set(["ambience", "melody", "tension", "combat", ...editor.stems])];
    const stem = el("div", "sf");
    stem.innerHTML = `<span class="nf-label">Stem</span><select>${stems.map((s) => `<option value="${esc(s)}"${s === stemOf(track) ? " selected" : ""}>${esc(STEM_NAMES[s] ?? s)}</option>`).join("")}</select>`;
    stem.querySelector("select")!.addEventListener("change", (e) => set("stem", () => (editor.music.tracks[t].stem = (e.target as HTMLSelectElement).value)));
    const patterns = patternNotes(editor.music, track).length;
    const actions = el("div", "track-actions");
    actions.innerHTML = `
      <button class="link" data-act="duplicate">Dupliquer</button>
      <button class="link" data-act="delete" ${editor.music.tracks.length <= 1 ? "disabled" : ""}>Supprimer</button>
      ${patterns ? `<button class="link" data-act="detach" title="Les notes jouées par des motifs deviennent les notes de la piste, modifiables une à une">Détacher les motifs (${patterns} notes)</button>` : ""}`;
    actions.onclick = (e) => {
      const act = (e.target as HTMLElement).closest<HTMLElement>("[data-act]")?.dataset.act;
      if (act === "duplicate") {
        editor.edit(`${track.name} dupliquée`, (take) => {
          const copy = JSON.parse(JSON.stringify(take.music.tracks[t]));
          copy.name = uniqueName(take.music.tracks.map((x) => x.name), track.name);
          take.music.tracks.splice(t + 1, 0, copy);
        });
        editor.select(t + 1);
      } else if (act === "delete") {
        editor.select(Math.max(0, t - 1));
        editor.edit(`${track.name} supprimée`, (take) => void take.music.tracks.splice(t, 1));
      } else if (act === "detach") {
        set("motifs détachés", () => detachPatterns(editor.music, editor.music.tracks[t]));
      }
    };
    root.append(
      block("Piste", [
        name,
        instrumentSelect(editor, t),
        stem,
        field("Volume", { value: track.volume_db, min: -30, max: 18, unit: "dB", reset: 0, hint: "0 dB : une note jouée à fond sort au niveau de référence, quel que soit l'instrument." }, `${track.name} · volume`, (v) => (editor.music.tracks[t].volume_db = v)),
        field("Placement", { value: track.pan, min: -1, max: 1, reset: 0, floor: -1, ceil: 1, hint: "-1 : à gauche, 1 : à droite." }, `${track.name} · placement`, (v) => (editor.music.tracks[t].pan = v)),
        field("Espace", { value: track.reverb, min: 0, max: 1, reset: 0.25, floor: 0, ceil: 1, hint: "Part envoyée dans la réverbération commune du morceau." }, `${track.name} · espace`, (v) => (editor.music.tracks[t].reverb = v)),
        field("Transposer", { value: track.transpose, min: -24, max: 24, integer: true, unit: "dt", reset: 0, hint: "Toute la piste, en demi-tons (12 = une octave)." }, `${track.name} · transposition`, (v) => (editor.music.tracks[t].transpose = v)),
        field("Jeu humain", { value: track.humanize, min: 0, max: 1, reset: 0.3, floor: 0, ceil: 1, hint: "Petites variations de moment et de force : 0 = mécanique." }, `${track.name} · jeu humain`, (v) => (editor.music.tracks[t].humanize = v)),
        actions,
      ]),
    );

    const index = editor.note;
    const note = index === null ? null : track.notes[index];
    if (note && index !== null) {
      const midi = midiOf(note.note);
      root.append(
        block(`Note ${noteName(midi)} · ${frenchName(midi)}`, [
          field("Force", { value: note.vel, min: 0.05, max: 1, reset: 0.7, floor: 0.01, ceil: 1, hint: "Nuance : plus doux à gauche, plus fort à droite." }, "Force de la note", (v) => (editor.music.tracks[t].notes[index].vel = v)),
          field("Durée", { value: note.len, min: 0.125, max: 16, scale: "sq", unit: "temps", floor: 0.0625 }, "Durée de la note", (v) => (editor.music.tracks[t].notes[index].len = v)),
          field("Début", { value: note.at, min: 0, max: editor.music.bars * editor.music.beats_per_bar, unit: "temps", floor: 0 }, "Début de la note", (v) => (editor.music.tracks[t].notes[index].at = v)),
        ]),
      );
    } else {
      root.append(el("p", "muted-note", "Clique une note pour régler sa force et sa durée. Clic dans le vide : une note ; double-clic : la supprimer."));
    }
  }

  const m = editor.music;
  const reverb = m.reverb;
  root.append(
    block(
      "Espace commun",
      reverb
        ? [
            field("Pièce", { value: reverb.room, min: 0, max: 1, floor: 0, ceil: 1, hint: "0 = petite pièce, 1 = cathédrale." }, "Espace · pièce", (v) => (editor.music.reverb!.room = v)),
            field("Amorti", { value: reverb.damping, min: 0, max: 1, floor: 0, ceil: 1, hint: "Étouffe les aigus de la queue." }, "Espace · amorti", (v) => (editor.music.reverb!.damping = v)),
            field("Mélange", { value: reverb.mix, min: 0, max: 1, floor: 0, ceil: 1 }, "Espace · mélange", (v) => (editor.music.reverb!.mix = v)),
          ]
        : [],
      { on: !!reverb, onChange: (on) => editor.edit(on ? "Espace commun activé" : "Espace commun retiré", (take) => (take.music.reverb = on ? { room: 0.6, damping: 0.5, mix: 0.25 } : null)) },
      "La réverbération partagée : chaque piste y envoie sa part (« Espace »).",
    ),
    block("Morceau", [
      field("Crête", { value: m.peak_db, min: -18, max: 0, unit: "dB", ceil: 0, floor: -40, reset: -1, hint: "Niveau de crête du mélange de tous les stems." }, "Niveau du morceau", (v) => (editor.music.peak_db = v)),
      field("Graine", { value: editor.doc.take.seed, min: 0, max: 9999, integer: true, floor: 0, hint: "Tire le jeu humain : même graine, mêmes fichiers." }, "Graine", (v) => (editor.doc.take.seed = v)),
      segmented([["loop", "En boucle"], ["once", "Une fois (jingle)"]], m.looping ? "loop" : "once", (v) => editor.edit(v === "loop" ? "En boucle" : "Une fois", (take) => (take.music.looping = v === "loop"))),
    ]),
  );

  const history = el("section", "block history");
  history.innerHTML = `<header><h3>Historique</h3></header><ol>${doc.steps
    .map((step, i) => `<li class="${step.author}${i === doc.index ? " current" : ""}${i > doc.index ? " undone" : ""}" data-step="${i}">${step.author === "claude" ? icon("sparkle", 12) : ""}<span>${esc(step.label)}</span></li>`)
    .reverse()
    .join("")}</ol>`;
  history.onclick = (e) => {
    const step = (e.target as HTMLElement).closest<HTMLElement>("[data-step]")?.dataset.step;
    if (step !== undefined) editor.gotoStep(Number(step));
  };
  root.append(history);
  root.scrollTop = scroll;
}

/** Instrument de la piste : ceux du catalogue par famille, ou un son de la bibliothèque joué comme instrument. */
function instrumentSelect(editor: MusicEditor, t: number): HTMLElement {
  const track = editor.music.tracks[t];
  const families = new Map<string, InstrumentInfo[]>();
  for (const i of editor.instruments) families.set(i.family, [...(families.get(i.family) ?? []), i]);
  const current = typeof track.instrument === "string" ? track.instrument : "__synth";
  const sounds = (editor.soundLibrary?.() ?? []).filter((s) => s.editable);
  const root = el("label", "sf instrument");
  root.innerHTML = `<span class="nf-label">Instrument</span><select>
    ${typeof track.instrument === "object" ? `<option value="__synth" selected>${esc(instrumentLabel(track.instrument, editor.instruments))}</option>` : ""}
    ${[...families].map(([family, list]) => `<optgroup label="${esc(family)}">${list.map((i) => `<option value="${esc(i.id)}"${i.id === current ? " selected" : ""}>${esc(i.name)}</option>`).join("")}</optgroup>`).join("")}
    ${sounds.length ? `<optgroup label="Sons de Sound Forge (synthèse)">${sounds.map((s) => `<option value="sound:${esc(s.name)}">${esc(s.name)}</option>`).join("")}</optgroup>` : ""}
  </select>`;
  root.querySelector("select")!.addEventListener("change", async (e) => {
    const value = (e.target as HTMLSelectElement).value;
    if (value === "__synth") return;
    if (value.startsWith("sound:")) {
      const recipe = await native.loadRecipe(value.slice(6));
      editor.edit(`${track.name} · instrument : son ${recipe.name}`, (take) => (take.music.tracks[t].instrument = { synth: { ...(recipe.sound as object), name: recipe.name } as never }));
    } else {
      editor.edit(`${track.name} · instrument : ${value}`, (take) => (take.music.tracks[t].instrument = value));
    }
  });
  return root;
}

export function uniqueName(taken: string[], base: string): string {
  const clean = base.replace(/[^A-Za-z0-9_-]/g, "_") || "piste";
  if (!taken.includes(clean)) return clean;
  let k = 2;
  while (taken.includes(`${clean}_${k}`)) k++;
  return `${clean}_${k}`;
}
