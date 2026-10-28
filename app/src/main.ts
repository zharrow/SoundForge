/** Point d'entrée : assemble l'éditeur, les panneaux, les menus natifs, le clavier et le pont avec Claude. */

import { installBridge } from "./bridge";
import { createCommands } from "./commands";
import { Editor } from "./editor";
import { validName } from "./model";
import { createMusicCommands } from "./music-commands";
import { MusicEditor } from "./music-editor";
import { renderMusicInspector, renderMusicLibrary, renderMusicTopbar, renderPieceBar, renderTracks } from "./ui/music";
import { PianoRoll } from "./ui/pianoroll";
import * as native from "./native";
import { esc, icon } from "./ui/icons";
import { renderLayers } from "./ui/layers";
import { MUSIC_HINT, SOUND_HINT, StatusBar, renderHistory, renderInspector, renderLibrary, renderTopbar } from "./ui/panels";
import { renderMacros } from "./ui/macros";
import { Preview } from "./ui/preview";
import { renderVariations } from "./ui/variations";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

const editor = new Editor();
const status = new StatusBar($("status"));
const commands = createCommands(editor, status, askName);
const preview = new Preview($("preview"), editor);
const music = new MusicEditor();
const musicCommands = createMusicCommands(music, status, askName);
const roll = new PianoRoll($("music-roll"), music);
music.soundLibrary = () => editor.library?.sounds ?? [];

// ---------- Onglets Sons / Musique ----------

type Mode = "sounds" | "music";
let mode: Mode = readMode();

function readMode(): Mode {
  try {
    return localStorage.getItem("mode") === "music" ? "music" : "sounds";
  } catch {
    return "sounds";
  }
}

function setMode(next: Mode): void {
  mode = next;
  try {
    localStorage.setItem("mode", next);
  } catch {}
  (next === "music" ? editor.player : music.player).stop();
  applyMode();
  if (next === "music" && !music.library) void music.refreshLibrary().catch(reportError);
}

function applyMode(): void {
  const musicOn = mode === "music";
  for (const id of ["topbar", "library", "main", "inspector"]) $(id).hidden = musicOn;
  for (const id of ["music-topbar", "music-library", "music-main", "music-inspector"]) $(id).hidden = !musicOn;
  status.hint = musicOn ? MUSIC_HINT : SOUND_HINT;
  status.render();
  $("mode-switch").innerHTML = `<button data-mode="sounds" class="${musicOn ? "" : "on"}">Sons</button><button data-mode="music" class="${musicOn ? "on" : ""}">Musique</button>`;
  if (musicOn) music.emit("doc");
  else editor.emit("doc");
}

$("mode-switch").addEventListener("click", (e) => {
  const next = (e.target as HTMLElement).closest<HTMLElement>("[data-mode]")?.dataset.mode as Mode | undefined;
  if (next && next !== mode) setMode(next);
});

const musicPending = new Set<string>();
let musicFrame = 0;
music.on((change) => {
  musicPending.add(change);
  if (!musicFrame) musicFrame = requestAnimationFrame(flushMusic);
});

function flushMusic(): void {
  musicFrame = 0;
  if (mode !== "music") return musicPending.clear();
  const all = musicPending.has("doc");
  const selection = musicPending.has("selection");
  if (all) {
    renderPieceBar($("piece-bar"), music);
  }
  if (all || selection || musicPending.has("history")) {
    renderTracks($("music-tracks"), music);
    renderMusicInspector($("music-inspector"), music, musicCommands);
    roll.render();
  }
  if (all || musicPending.has("library")) renderMusicLibrary($("music-library"), music, musicCommands);
  if (all || musicPending.has("render") || musicPending.has("play") || musicPending.has("history") || musicPending.has("value")) {
    renderMusicTopbar($("music-topbar"), music);
  }
  if (musicPending.has("render") && !all) renderMusicInspector($("music-inspector"), music, musicCommands);
  musicPending.clear();
  const { doc } = music;
  void native.setTitle(`${doc.dirty ? "• " : ""}${doc.name ?? "Morceau sans nom"} — Sound Forge`);
}

// ---------- Rendu de l'interface ----------

const pending = new Set<string>();
let frame = 0;
editor.on((change) => {
  pending.add(change);
  if (!frame) frame = requestAnimationFrame(flush);
});

function flush(): void {
  frame = 0;
  if (mode !== "sounds") return pending.clear();
  const all = pending.has("doc");
  if (all) {
    renderLayers($("layer-list"), editor);
    $("assist").hidden = !editor.simple;
    if (editor.simple) {
      renderMacros($("macros"), editor);
      renderVariations($("variations"), editor);
    }
    renderInspector($("inspector"), editor, commands);
  } else if (pending.has("history") || pending.has("value")) {
    renderHistory($("inspector"), editor);
  }
  if (all || pending.has("value") || pending.has("history") || pending.has("render") || pending.has("play")) {
    renderTopbar($("topbar"), editor, commands);
    restoreBadge();
  }
  if (all || pending.has("library")) renderLibrary($("library"), editor, commands);
  if (all || pending.has("render") || pending.has("value")) preview.render();
  pending.clear();
  const { doc } = editor;
  void native.setTitle(`${doc.dirty ? "• " : ""}${doc.name ?? "Son sans nom"} — Sound Forge`);
}

// ---------- Activité de Claude ----------

let badgeText = "";
let badgeTimer = 0;
function notifyClaude(label: string): void {
  status.show(`Claude · ${label}`, true);
  badgeText = label;
  restoreBadge(true);
  const main = $("main");
  main.classList.remove("claude-flash");
  void main.offsetWidth;
  main.classList.add("claude-flash");
  window.clearTimeout(badgeTimer);
  badgeTimer = window.setTimeout(() => {
    badgeText = "";
    restoreBadge();
  }, 2600);
}

function restoreBadge(fresh = false): void {
  const badge = document.getElementById("claude-badge");
  if (!badge) return;
  badge.innerHTML = badgeText ? `${icon("sparkle", 13)} Claude · ${esc(badgeText)}` : "";
  badge.classList.toggle("visible", !!badgeText);
  if (fresh) badge.classList.add("pulse");
}

// ---------- Enregistrer sous ----------

function askName(initial: string): Promise<string | null> {
  const dialog = $<HTMLDialogElement>("name-dialog");
  const input = dialog.querySelector<HTMLInputElement>("input")!;
  const error = dialog.querySelector<HTMLElement>(".error")!;
  input.value = initial;
  error.textContent = "";
  dialog.showModal();
  input.select();
  return new Promise((resolve) => {
    const finish = (value: string | null) => {
      dialog.close();
      dialog.onsubmit = null;
      dialog.oncancel = null;
      resolve(value);
    };
    dialog.onsubmit = (e) => {
      e.preventDefault();
      if ((e.submitter as HTMLButtonElement | null)?.value === "cancel") return finish(null);
      const name = input.value.trim();
      if (!validName(name)) {
        error.textContent = "Lettres, chiffres, _ et - uniquement (64 au plus).";
        return;
      }
      finish(name);
    };
    dialog.oncancel = () => finish(null);
  });
}

// ---------- Clavier et menus ----------

const typing = () => {
  const el = document.activeElement as HTMLElement | null;
  return !!el && ((el.tagName === "INPUT" && (el as HTMLInputElement).type !== "range") || el.tagName === "TEXTAREA" || el.tagName === "SELECT");
};

window.addEventListener("keydown", (e) => {
  if (document.querySelector("dialog[open]") || e.metaKey || e.ctrlKey || e.altKey) return;
  if (e.key === "Escape") return (document.activeElement as HTMLElement | null)?.blur();
  if (typing()) return;
  if (mode === "music") {
    if (e.code === "Space") {
      e.preventDefault();
      music.player.toggle();
    } else if (roll.key(e)) {
      e.preventDefault();
    }
    return;
  }
  if (e.code === "Space") {
    e.preventDefault();
    commands.play();
  } else if (/^[1-9]$/.test(e.key) && Number(e.key) <= editor.doc.take.variants) {
    editor.setVariant(Number(e.key) - 1);
  }
});

const TEXT_EDIT: Record<string, string> = { undo: "undo", redo: "redo", cut: "cut", copy: "copy", paste: "paste", select_all: "selectAll" };

native.onMenu((id) => {
  if (typing() && TEXT_EDIT[id]) {
    document.execCommand(TEXT_EDIT[id]);
    return;
  }
  if (document.querySelector("dialog[open]")) return;
  // Les commandes communes (lecture, annulation, enregistrement) visent l'onglet ouvert.
  const set = mode === "music" && musicCommands[id] ? musicCommands : commands;
  Promise.resolve(set[id]?.()).catch(reportError);
});

native.onLibraryChanged((names) => {
  const pieces = names.filter((n) => n.startsWith("musique/")).map((n) => n.slice("musique/".length));
  if (pieces.length) {
    void music.refreshLibrary().catch(() => {});
    const own = Date.now() - music.savedAt < 2500;
    if (music.doc.name && pieces.includes(music.doc.name) && !music.doc.dirty && !own) {
      void musicCommands.reloadFromDisk("Modifié sur le disque", "user").catch(() => {});
    }
  }
  names = names.filter((n) => !n.startsWith("musique/"));
  if (!names.length) return;
  void editor.refreshLibrary(names);
  const current = editor.doc.name;
  const ownSave = Date.now() - editor.savedAt < 2500;
  if (current && names.includes(current) && !editor.doc.dirty && !ownSave) {
    // Le son ouvert a changé sur le disque : la nouvelle version entre dans l'historique s'il n'y a rien à perdre.
    void commands
      .reloadFromDisk("Modifié sur le disque", "user")
      .then((changed) => changed && status.show(`« ${current} » a changé sur le disque : rechargé (⌘Z pour revenir).`))
      .catch(() => {});
  }
});

native.onCloseRequested(async () => {
  const unsaved = [editor.doc.dirty && (editor.doc.name ?? "le son sans nom"), music.doc.dirty && (music.doc.name ?? "le morceau sans nom")].filter(Boolean);
  return !unsaved.length || native.confirm(`Modifications non enregistrées : ${unsaved.join(", ")}. Quitter quand même ?`, "Quitter Sound Forge", "Quitter sans enregistrer");
});

/** Dernières erreurs, lisibles par le pont (méthode « diagnostics ») : Claude ne voit pas l'écran. */
export const errors: string[] = [];

function reportError(error: unknown): void {
  const text = error instanceof Error ? `${error.message}\n${error.stack ?? ""}` : String(error);
  errors.push(text);
  if (errors.length > 20) errors.shift();
  void native.alertError(text);
}
window.addEventListener("unhandledrejection", (e) => reportError(e.reason));
// Une erreur de dessin ne doit jamais passer en silence : l'interface resterait vide sans explication.
window.addEventListener("error", (e) => reportError(e.error ? `${e.error.message}\n${e.error.stack ?? ""}` : e.message));

/** Un morceau rendu par Claude (render_music) : il s'ouvre dans l'onglet Musique s'il n'y a rien à perdre. */
async function showMusic(name: string): Promise<{ opened: boolean; reason?: string }> {
  await music.refreshLibrary();
  if (music.doc.dirty) return { opened: false, reason: "le morceau ouvert a des modifications non enregistrées : il reste ouvert" };
  setMode("music");
  if (name === music.doc.name) {
    await musicCommands.reloadFromDisk(`Rendu de ${name}`, "claude");
    return { opened: true, reason: "nouvelle version du morceau ouvert, annulable avec ⌘Z" };
  }
  await musicCommands.openPiece(name, false);
  return { opened: true };
}

installBridge(editor, commands, notifyClaude, () => ({
  errors,
  viewport: [window.innerWidth, window.innerHeight],
  areas: Object.fromEntries(
    ["app", "topbar", "library", "main", "preview", "layers", "assist", "layer-list", "inspector", "status"].map((id) => {
      const r = document.getElementById(id)?.getBoundingClientRect();
      return [id, r ? [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)] : null];
    }),
  ),
  scroll: [document.documentElement.scrollWidth, document.documentElement.scrollHeight],
  visibility: document.visibilityState,
  mode,
  music: {
    name: music.doc.name,
    tracks: music.music.tracks.length,
    rendering: music.rendering,
    error: music.renderError,
    stems: music.rendered?.stems ?? null,
    rerendered: music.rendered?.rendered ?? null,
    history: music.doc.steps.map((s) => `${s.author}: ${s.label}`),
  },
}), showMusic);
status.render();
applyMode();
editor.requestRender(false);
music.requestRender();
void editor.refreshLibrary().catch(reportError);
void music.refreshLibrary().catch(reportError);
