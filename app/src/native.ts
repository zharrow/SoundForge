/** Accès au côté natif (Tauri) : moteur, dossier de travail, menus, boîtes de dialogue, titre. */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, message } from "@tauri-apps/plugin-dialog";
import type { Sound, Take } from "./model";
import type { InstrumentInfo, Music, MusicTake, Voice } from "./music-model";

export interface Rendered {
  wav: string;
  spectrum: string;
  overview: string | null;
  duration: number;
  peak_db: number;
  rms_db: number;
  bands: [string, number][];
  layers: [number, number][];
  count: number;
  suffix: string;
}

export interface LibraryEntry {
  name: string;
  editable: boolean;
  variants: number;
  files: string[];
  preset: string | null;
  generator: string | null;
  modified: number;
}

export interface Library {
  folder: string;
  sounds: LibraryEntry[];
  presets: [string, string][];
  /** Enregistrements (« voix/souffle ») et leur durée en secondes. */
  recordings: [string, number][];
}

export interface Opened {
  name: string;
  sound: unknown;
  variants: number;
  amount: number;
  seed: number;
  preset: string | null;
  generator: string;
}

export function isNative(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export const render = (take: Take, variant: number, overview = false) => invoke<Rendered>("render", { take, variant, overview });
export const library = () => invoke<Library>("library");
export const loadRecipe = (name: string) => invoke<Opened>("load_recipe", { name });
export const preset = (name: string) => invoke<Sound>("preset", { name });
export const save = (name: string, take: Take, preset: string | null) => invoke<{ files: string[] }>("save", { name, take, preset });
export const exists = (name: string) => invoke<boolean>("exists", { name });
export const reveal = (name: string | null) => invoke<void>("reveal", { name });
export const saveRecording = (name: string, wav: string) => invoke<string>("save_recording", { name, wav });
export const recordingWav = (name: string) => invoke<string>("recording_wav", { name });

// ---------- Musique ----------

export interface Piece {
  name: string;
  bpm: number;
  bars: number;
  tracks: number;
  modified: number;
}

export interface MusicLibrary {
  pieces: Piece[];
  instruments: InstrumentInfo[];
  catalogue_error: string | null;
}

export interface RenderedMusic {
  stems: [string, number][];
  duration: number;
  frames: number;
  rendered: number;
  spectrum: string;
}

export const musicLibrary = () => invoke<MusicLibrary>("music_library");
export const loadMusic = (name: string) => invoke<MusicTake & { generator: string; files: string[] }>("load_music", { name });
export const renderMusicTake = (music: Music, seed: number) => invoke<RenderedMusic>("render_music_take", { music, seed });
/** Un stem du dernier rendu, en WAV binaire. */
export const musicStem = (name: string) => invoke<ArrayBuffer>("music_stem", { name });
export const previewNote = (instrument: Voice, note: number, vel: number, seconds: number) =>
  invoke<ArrayBuffer>("preview_note", { instrument, note, vel, seconds });
export const saveMusic = (name: string, music: Music, seed: number) => invoke<string[]>("save_music", { name, music, seed });
export const musicExists = (name: string) => invoke<boolean>("music_exists", { name });

export async function confirm(text: string, title = "Sound Forge", okLabel = "Continuer"): Promise<boolean> {
  if (!isNative()) return window.confirm(text);
  return ask(text, { title, kind: "warning", okLabel, cancelLabel: "Annuler" });
}

export async function alertError(text: string): Promise<void> {
  if (!isNative()) return window.alert(text);
  await message(text, { title: "Sound Forge", kind: "error" });
}

export async function setTitle(title: string): Promise<void> {
  document.title = title;
  if (isNative()) await getCurrentWindow().setTitle(title);
}

export function onMenu(handler: (id: string) => void): void {
  if (isNative()) listen<string>("menu", (event) => handler(event.payload));
}

/** Sons du dossier ajoutés, modifiés ou retirés (noms de base), par Claude ou un autre outil. */
export function onLibraryChanged(handler: (names: string[]) => void): void {
  if (isNative()) listen<string[]>("library-changed", (event) => handler(event.payload));
}

/** Avant la fermeture : `canClose` peut demander confirmation (son non enregistré). */
export function onCloseRequested(canClose: () => Promise<boolean>): void {
  if (!isNative()) return;
  getCurrentWindow().onCloseRequested(async (event) => {
    if (!(await canClose())) event.preventDefault();
  });
}
