/**
 * Panneaux autour des couches : la barre d'écoute (en haut), la bibliothèque (à gauche), le son
 * (à droite : fichier, niveau, effets, variantes, historique) et la barre d'état.
 */

import type { Commands } from "../commands";
import type { Editor } from "../editor";
import { defaultEcho, defaultReverb } from "../model";
import { block, el, formatNumber, numberField, type FieldSpec, type Phase } from "./controls";
import { BLOCK_HELP } from "./help";
import { esc, icon } from "./icons";

// ---------- Barre d'écoute ----------

export function renderTopbar(root: HTMLElement, editor: Editor, commands: Commands): void {
  const { doc, player } = editor;
  const count = Math.max(1, doc.take.variants);
  root.innerHTML = `
    <div class="doc-title">
      ${icon("wave", 16)}
      <strong>${esc(doc.name ?? "Son sans nom")}</strong>
      ${doc.dirty ? '<span class="dirty" title="Modifications non enregistrées">●</span>' : ""}
      ${doc.preset ? `<span class="tag">preset ${esc(doc.preset)}</span>` : ""}
    </div>
    <div class="transport">
      <button class="play-button${player.playing ? " on" : ""}" data-cmd="play" title="Écouter / arrêter (Espace)">${icon(player.playing ? "stop" : "play", 16)}</button>
      <button class="chip" data-cmd="before" title="Écouter la version ${doc.name ? "enregistrée" : "de départ"}, avant tes retouches" ${doc.dirty ? "" : "disabled"}>Avant</button>
      <button class="chip" data-cmd="compare" title="Avant puis après, à la suite" ${doc.dirty ? "" : "disabled"}>${icon("compare", 13)} Comparer</button>
      <button class="chip${player.loop ? " on" : ""}" data-toggle="loop" title="En boucle : une retouche s'entend au tour suivant">${icon("loop", 13)} Boucle</button>
      <button class="chip${editor.autoplay ? " on" : ""}" data-toggle="autoplay" title="Écouter après chaque retouche">${icon("volume", 13)} Auto</button>
    </div>
    <div class="variants" title="Variantes : 1 à 9 au clavier, ⌘← ⌘→">
      ${count > 1 ? Array.from({ length: count }, (_, i) => `<button class="variant${i === editor.variant ? " on" : ""}" data-variant="${i}">${String(i + 1).padStart(2, "0")}</button>`).join("") : '<span class="muted">une seule version</span>'}
    </div>
    <span class="spacer"></span>
    <div id="claude-badge" class="claude-badge"></div>
    <button class="chip record" data-cmd="record" title="Enregistrer ta voix (⌘R)">${icon("mic", 13)} Enregistrer ma voix</button>
    <div class="segmented mode" title="Simple : réglages en mots et l'essentiel de chaque couche. Détaillé : tous les réglages.">
      <button data-mode="simple" class="${editor.simple ? "on" : ""}">Simple</button>
      <button data-mode="detail" class="${editor.simple ? "" : "on"}">Détaillé</button>
    </div>`;
  root.onclick = (e) => {
    const target = e.target as HTMLElement;
    const variant = target.closest<HTMLElement>("[data-variant]")?.dataset.variant;
    const toggle = target.closest<HTMLElement>("[data-toggle]")?.dataset.toggle;
    const mode = target.closest<HTMLElement>("[data-mode]")?.dataset.mode;
    const cmd = target.closest<HTMLElement>("[data-cmd]")?.dataset.cmd;
    if (mode) editor.setSimple(mode === "simple");
    else if (cmd && cmd !== "play") void commands[cmd]?.();
    else if (variant !== undefined) editor.setVariant(Number(variant));
    else if (toggle === "loop") editor.setLoop(!player.loop);
    else if (toggle === "autoplay") editor.setAutoplay(!editor.autoplay);
    else if (target.closest("[data-cmd=play]")) commands.play();
  };
}

// ---------- Bibliothèque ----------

let query = "";
let presetsOpen = false;

export function renderLibrary(root: HTMLElement, editor: Editor, commands: Commands): void {
  const lib = editor.library;
  const focused = document.activeElement?.classList.contains("search-input");
  const q = query.trim().toLowerCase();
  const sounds = (lib?.sounds ?? []).filter((s) => !q || s.name.toLowerCase().includes(q));
  const presets = (lib?.presets ?? []).filter(([name, d]) => !q || name.includes(q) || d.toLowerCase().includes(q));
  const now = Date.now();
  root.innerHTML = `
    <div class="library-head">
      <h2>Sons</h2>
      <button class="icon-button" data-cmd="new" title="Nouveau son (⌘N)">${icon("plus", 16)}</button>
    </div>
    <label class="search">${icon("search", 14)}<input class="search-input" type="search" placeholder="Chercher" value="${esc(query)}"></label>
    <div class="library-list">
      ${sounds.length ? sounds.map((s) => `
        <button class="lib-item${s.name === editor.doc.name ? " current" : ""}${s.editable ? "" : " locked"}${now - (editor.fresh.get(s.name) ?? 0) < 8000 ? " fresh" : ""}" data-sound="${esc(s.name)}" ${s.editable ? "" : 'title="Pas de recette : ce WAV ne se retouche pas (rendu avant SoundForge 0.4)."'}>
          <span class="lib-name">${esc(s.name)}</span>
          <span class="lib-meta">${s.editable ? "" : icon("lock", 11)}${s.variants > 1 ? `×${s.variants}` : ""}</span>
        </button>`).join("") : `<p class="muted-note">${lib ? "Aucun son." : "Chargement…"}</p>`}
      <div class="lib-section static">${icon("mic", 12)} Voix <button class="icon-button tiny" data-cmd="record" title="Enregistrer ta voix (⌘R)">${icon("plus", 13)}</button></div>
      ${(lib?.recordings ?? []).length ? (lib?.recordings ?? []).map(([name, seconds]) => `
        <div class="lib-item voice" data-recording="${esc(name)}" title="Clic : écouter. + : ajouter comme couche au son ouvert. Claude peut l'écouter sous ce nom.">
          <span class="lib-name">${esc(name.replace(/^voix\//, ""))}</span>
          <span class="lib-meta">${formatNumber(seconds)} s</span>
          <button class="icon-button tiny" data-add-recording="${esc(name)}" title="Ajouter comme couche">${icon("plus", 12)}</button>
        </div>`).join("") : '<p class="muted-note">Pas encore d\'enregistrement.</p>'}
      <button class="lib-section" data-toggle-presets>${icon(presetsOpen || q ? "chevronDown" : "chevronRight", 13)} Presets</button>
      ${presetsOpen || q ? presets.map(([name, description]) => `
        <button class="lib-item preset" data-preset="${esc(name)}" title="${esc(description)}">
          <span class="lib-name">${esc(name)}</span>
        </button>`).join("") : ""}
    </div>
    <button class="library-folder" data-cmd="reveal_folder" title="Afficher le dossier dans le Finder">${icon("folder", 13)} <span>${esc((lib?.folder ?? "").replace(/^\/Users\/[^/]+/, "~"))}</span></button>`;
  const input = root.querySelector<HTMLInputElement>(".search-input")!;
  input.addEventListener("input", () => {
    query = input.value;
    renderLibrary(root, editor, commands);
  });
  if (focused) {
    input.focus();
    input.setSelectionRange(query.length, query.length);
  }
  root.onclick = (e) => {
    const target = e.target as HTMLElement;
    const sound = target.closest<HTMLElement>("[data-sound]");
    const presetName = target.closest<HTMLElement>("[data-preset]")?.dataset.preset;
    const cmd = target.closest<HTMLElement>("[data-cmd]")?.dataset.cmd;
    const addRecording = target.closest<HTMLElement>("[data-add-recording]")?.dataset.addRecording;
    const recording = target.closest<HTMLElement>("[data-recording]")?.dataset.recording;
    if (addRecording) commands.addRecording(addRecording);
    else if (recording) void commands.playRecording(recording);
    else if (sound && !sound.classList.contains("locked")) void commands.openSound(sound.dataset.sound!);
    else if (presetName) void commands.openPreset(presetName);
    else if (target.closest("[data-toggle-presets]")) {
      presetsOpen = !presetsOpen;
      renderLibrary(root, editor, commands);
    } else if (cmd) void commands[cmd]?.();
  };
}

// ---------- Le son : fichier, niveau, effets, variantes, historique ----------

export function renderInspector(root: HTMLElement, editor: Editor, commands: Commands): void {
  const scroll = root.scrollTop;
  const { doc } = editor;
  const take = doc.take;
  const fx = take.sound.effects;
  root.innerHTML = "";
  const field = (label: string, spec: Omit<FieldSpec, "label" | "onChange">, history: string, set: (v: number) => void) =>
    numberField({ ...spec, label, onChange: (v: number, phase: Phase) => editor.gesture(history, phase, () => set(v)) });
  const edit = (label: string, change: () => void) => editor.edit(label, change);

  const file = el("section", "file-card");
  file.innerHTML = `
    <div class="file-name">${doc.name ? esc(doc.name) : '<span class="muted">jamais enregistré</span>'}${doc.dirty ? ' <span class="dirty">●</span>' : ""}</div>
    <div class="file-actions">
      <button class="button primary" data-cmd="save" title="⌘S">${icon("save", 14)} Enregistrer</button>
      <button class="button" data-cmd="save_as" title="⇧⌘S">Enregistrer sous…</button>
    </div>
    <div class="file-actions secondary">
      <button class="link" data-cmd="revert" ${doc.name && doc.dirty ? "" : "disabled"}>Revenir à l'enregistré</button>
      <button class="link" data-cmd="reveal" ${doc.name ? "" : "disabled"}>Finder</button>
    </div>`;
  file.onclick = (e) => {
    const cmd = (e.target as HTMLElement).closest<HTMLElement>("[data-cmd]")?.dataset.cmd;
    if (cmd) void commands[cmd]?.();
  };
  root.append(file);

  root.append(
    block("Niveau", [
      field("Crête", { value: take.sound.peak_db, min: -24, max: 0, unit: "dB", ceil: 0, floor: -60, reset: -1, hint: "Niveau de crête après normalisation : -1 pour un impact fort, -6 à -13 pour l'interface ou un son discret." }, "Crête", (v) => (editor.doc.take.sound.peak_db = v)),
    ], undefined, BLOCK_HELP.Niveau),
  );

  root.append(
    block(
      "Saturation",
      fx.drive === null ? [] : [field("Dose", { value: fx.drive, min: 0.1, max: 6, reset: 1.5, floor: 0, hint: "1 = légère, 4 = très agressive." }, "Saturation", (v) => (editor.doc.take.sound.effects.drive = v))],
      { on: fx.drive !== null, onChange: (on) => edit(on ? "Saturation activée" : "Saturation retirée", () => (editor.doc.take.sound.effects.drive = on ? 1.5 : null)) },
      BLOCK_HELP.Saturation,
    ),
    block(
      "Écho",
      fx.echo === null
        ? []
        : [
            field("Délai", { value: fx.echo.time, min: 0.01, max: 1, scale: "sq", unit: "s", floor: 0.001 }, "Écho · délai", (v) => (editor.doc.take.sound.effects.echo!.time = v)),
            field("Retour", { value: fx.echo.feedback, min: 0, max: 0.9, floor: 0, ceil: 0.9, hint: "Part réinjectée : le nombre de répétitions." }, "Écho · retour", (v) => (editor.doc.take.sound.effects.echo!.feedback = v)),
            field("Mélange", { value: fx.echo.mix, min: 0, max: 1, floor: 0, ceil: 1 }, "Écho · mélange", (v) => (editor.doc.take.sound.effects.echo!.mix = v)),
          ],
      { on: fx.echo !== null, onChange: (on) => edit(on ? "Écho activé" : "Écho retiré", () => (editor.doc.take.sound.effects.echo = on ? defaultEcho() : null)) },
      BLOCK_HELP.Écho,
    ),
    block(
      "Réverbération",
      fx.reverb === null
        ? []
        : [
            field("Pièce", { value: fx.reverb.room, min: 0, max: 1, floor: 0, ceil: 1, hint: "0 = placard, 1 = cathédrale." }, "Réverbération · pièce", (v) => (editor.doc.take.sound.effects.reverb!.room = v)),
            field("Amorti", { value: fx.reverb.damping, min: 0, max: 1, floor: 0, ceil: 1, hint: "Étouffe les aigus de la queue." }, "Réverbération · amorti", (v) => (editor.doc.take.sound.effects.reverb!.damping = v)),
            field("Mélange", { value: fx.reverb.mix, min: 0, max: 1, floor: 0, ceil: 1, hint: "0,1 à 0,3 en général." }, "Réverbération · mélange", (v) => (editor.doc.take.sound.effects.reverb!.mix = v)),
          ],
      { on: fx.reverb !== null, onChange: (on) => edit(on ? "Réverbération activée" : "Réverbération retirée", () => (editor.doc.take.sound.effects.reverb = on ? defaultReverb() : null)) },
      BLOCK_HELP.Réverbération,
    ),
  );

  const seedRow = el("div", "seed-row");
  seedRow.append(
    field("Graine", { value: take.seed, min: 0, max: 9999, integer: true, floor: 0, hint: "Même graine = mêmes variantes, octet pour octet." }, "Graine", (v) => (editor.doc.take.seed = v)),
  );
  const dice = el("button", "icon-button", icon("dice", 15));
  dice.title = "Nouvelle graine (⌘G)";
  dice.addEventListener("click", () => commands.new_seed());
  seedRow.append(dice);
  root.append(
    block("Variantes", [
      field("Nombre", { value: take.variants, min: 1, max: 16, integer: true, floor: 1, ceil: 16, reset: 1, hint: "1 = un seul fichier ; plus = nom_01.wav, nom_02.wav… Trois à cinq pour un geste fréquent." }, "Nombre de variantes", (v) => (editor.doc.take.variants = v)),
      field("Écart", { value: take.amount, min: 0, max: 1, floor: 0, ceil: 1, reset: 0.4, hint: "0 = identiques, 1 = très différentes (±2 demi-tons, durées ±25 %). 0,3 à 0,5 est un bon réglage." }, "Écart des variantes", (v) => (editor.doc.take.amount = v)),
      seedRow,
    ], undefined, BLOCK_HELP.Variantes),
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

export function renderHistory(root: HTMLElement, editor: Editor): void {
  const list = root.querySelector(".history ol");
  if (!list) return;
  const { doc } = editor;
  list.innerHTML = doc.steps
    .map((step, i) => `<li class="${step.author}${i === doc.index ? " current" : ""}${i > doc.index ? " undone" : ""}" data-step="${i}">${step.author === "claude" ? icon("sparkle", 12) : ""}<span>${esc(step.label)}</span></li>`)
    .reverse()
    .join("");
}

// ---------- Barre d'état ----------

export const SOUND_HINT = "Espace écouter · 1-9 variante · ⌘Z annuler · tire les points des dessins · double-clic sur un nom de réglage : valeur par défaut";
export const MUSIC_HINT = "Espace écouter · clic : une note · glisser : la déplacer · bord droit : la rallonger · double-clic ou Suppr : l'enlever · ↑↓ ←→ : la bouger · ⌘Z annuler";

export class StatusBar {
  private message = "";
  hint = SOUND_HINT;
  private timer = 0;

  constructor(private root: HTMLElement) {}

  show(text: string, claude = false): void {
    this.message = claude ? `<span class="status-claude">${icon("sparkle", 12)} ${esc(text)}</span>` : esc(text);
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => {
      this.message = "";
      this.render();
    }, 6000);
    this.render();
  }

  render(): void {
    this.root.innerHTML = `<span class="status-msg">${this.message}</span><span class="spacer"></span><span class="muted">${esc(this.hint)}</span>`;
  }
}

export { formatNumber };
