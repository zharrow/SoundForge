/**
 * Pont avec Claude : le serveur MCP envoie des appels { method, params } que Rust relaie ici
 * (événement « rpc ») ; chaque méthode agit sur le son ouvert et renvoie un résultat JSON.
 *
 * Les retouches de Claude entrent dans le même historique que celles de l'utilisateur (annulables
 * avec ⌘Z, marquées de son nom), et s'entendent tout de suite si l'écoute automatique est active.
 * Claude ne jette jamais un travail non enregistré : ouvrir un autre son dans ce cas est refusé,
 * sauf `force`.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Commands } from "./commands";
import { SoundDoc } from "./doc";
import type { Editor } from "./editor";
import { type Take, normalizeSound, normalizeTake, newTake } from "./model";
import * as native from "./native";

type Params = Record<string, unknown>;
type Handler = (params: Params) => unknown | Promise<unknown>;

export function installBridge(
  editor: Editor,
  commands: Commands,
  notify: (label: string) => void,
  diagnostics?: () => unknown,
  showMusic?: (name: string) => Promise<unknown>,
): void {
  const handlers = createHandlers(editor, commands, notify);
  // Un morceau (musique/<nom>) s'ouvre dans l'onglet Musique ; le reste suit le chemin des sons.
  const showSound = handlers.show;
  handlers.show = (p) => {
    const raw = typeof p.filename === "string" ? p.filename : "";
    if (showMusic && raw.startsWith("musique/")) {
      notify(`a rendu ${raw}`);
      return showMusic(raw.slice("musique/".length).replace(/\.json$/, ""));
    }
    return showSound(p);
  };
  // Mise en page et erreurs récentes : ce qui remplace l'écran pour qui ne le voit pas.
  if (diagnostics) handlers.diagnostics = diagnostics;
  if (!native.isNative()) return;
  listen<{ id: number; method: string; params: Params | null }>("rpc", async (event) => {
    const { id, method, params } = event.payload;
    let response: unknown;
    try {
      const handler = handlers[method];
      if (!handler) throw new Error(`Méthode inconnue : ${method}.`);
      response = { ok: true, result: (await handler(params ?? {})) ?? null };
    } catch (error) {
      response = { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
    await invoke("rpc_reply", { id, response });
  }).then(() => invoke("bridge_ready"));
}

export function createHandlers(editor: Editor, commands: Commands, notify: (label: string) => void): Record<string, Handler> {
  const str = (p: Params, key: string): string | undefined => {
    const value = p[key];
    if (value === undefined || value === null) return undefined;
    if (typeof value !== "string") throw new Error(`Paramètre '${key}' : texte attendu.`);
    return value;
  };
  const num = (p: Params, key: string): number | undefined => {
    const value = p[key];
    if (value === undefined || value === null) return undefined;
    if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`Paramètre '${key}' : nombre attendu.`);
    return value;
  };

  const guardDirty = (p: Params, action: string) => {
    if (editor.doc.dirty && p.force !== true) {
      throw new Error(
        `Le son ouvert (« ${editor.doc.name ?? "sans nom"} ») a des modifications non enregistrées : ${action} les perdrait. ` +
          "Enregistrer d'abord (save_sound), ou repasser avec force=true si l'utilisateur est d'accord.",
      );
    }
  };

  /** Le son d'un nom de fichier : « pas », « pas_02 », « pas_02.wav » ou « pas.json » donnent « pas ». */
  const soundName = (raw: string): string => {
    const stem = raw.replace(/\.(wav|json)$/, "");
    const sounds = editor.library?.sounds ?? [];
    if (sounds.some((s) => s.name === stem)) return stem;
    const base = stem.replace(/_\d\d$/, "");
    return sounds.some((s) => s.name === base) ? base : stem;
  };

  const summary = () => {
    const { doc } = editor;
    return {
      name: doc.name,
      preset: doc.preset,
      dirty: doc.dirty,
      variant: editor.variant + 1,
      take: doc.take,
      history: doc.steps.slice(Math.max(0, doc.index - 9), doc.index + 1).map((s) => `${s.author === "claude" ? "Claude" : "Utilisateur"} : ${s.label}`),
      muted_layers: [...editor.muted].map((i) => i + 1),
      solo_layers: [...editor.solo].map((i) => i + 1),
      folder: editor.library?.folder ?? null,
    };
  };

  /** Mesures et image de contrôle de la variante écoutée, avec TOUTES les couches (pas l'aperçu partiel). */
  const measure = async () => {
    await editor.settled();
    if (editor.renderError && !editor.partial) throw new Error(editor.renderError);
    const r = await native.render(editor.doc.take, editor.variant, true);
    return {
      file: `${editor.doc.name ?? "(sans nom)"}${r.suffix}.wav`,
      duration_s: round(r.duration),
      peak_db: round(r.peak_db),
      rms_db: round(r.rms_db),
      bands: Object.fromEntries(r.bands.map(([name, v]) => [name, round(v)])),
      overview_png: r.overview,
    };
  };

  return {
    get_document: () => summary(),

    async open_sound(p) {
      const filename = str(p, "filename");
      const presetName = str(p, "preset");
      if (!filename === !presetName) throw new Error("Donner soit `filename` (un son du dossier), soit `preset`.");
      await editor.refreshLibrary();
      const name = filename ? soundName(filename) : null;
      if (name !== editor.doc.name || presetName) guardDirty(p, "l'ouvrir");
      if (name) await commands.openSound(name, false);
      else await commands.openPreset(presetName!, false);
      notify(`ouvre ${filename ?? presetName}`);
      return { ...summary(), ...(await measure()) };
    },

    async new_sound(p) {
      guardDirty(p, "en créer un nouveau");
      const take: Take = p.sound ? normalizeTake({ sound: p.sound, variants: num(p, "variants"), amount: num(p, "amount"), seed: num(p, "seed") }) : newTake();
      editor.open(new SoundDoc(take, null, str(p, "preset") ?? null, "Nouveau son (Claude)"));
      notify("nouveau son");
      return { ...summary(), ...(await measure()) };
    },

    async edit_sound(p) {
      const sound = p.sound === undefined ? undefined : normalizeSound(p.sound);
      const variants = num(p, "variants");
      const amount = num(p, "amount");
      const seed = num(p, "seed");
      if (!sound && variants === undefined && amount === undefined && seed === undefined) {
        throw new Error("Rien à changer : donner `sound` et/ou `variants`, `amount`, `seed`.");
      }
      const label = str(p, "label") ?? "Retouche";
      const changed = editor.edit(
        label,
        (take) => {
          if (sound) take.sound = sound;
          if (variants !== undefined) take.variants = Math.min(16, Math.max(1, Math.round(variants)));
          if (amount !== undefined) take.amount = Math.min(1, Math.max(0, amount));
          if (seed !== undefined) take.seed = Math.max(0, Math.round(seed));
        },
        "claude",
      );
      notify(changed ? label : "aucun changement");
      // Claude vient d'envoyer la prise : inutile de la lui renvoyer.
      const { take: _take, ...state } = summary();
      return { changed, ...state, ...(await measure()) };
    },

    async play(p) {
      const variant = num(p, "variant");
      if (variant !== undefined) editor.setVariant(Math.round(variant) - 1, false);
      await editor.settled();
      editor.player.play();
      notify("écoute");
      return { playing: true, variant: editor.variant + 1, duration_s: editor.rendered ? round(editor.rendered.duration) : null };
    },

    async save_sound(p) {
      const filename = str(p, "filename") ?? editor.doc.name;
      if (!filename) throw new Error("Le son n'a jamais été enregistré : donner `filename`.");
      const files = await commands.saveAs(filename, p.overwrite === true || filename === editor.doc.name);
      notify(`enregistre ${filename}`);
      return { files, folder: editor.library?.folder ?? null };
    },

    /** Après un rendu par le serveur MCP (render_sound…) : le son apparaît, et s'ouvre si rien n'est perdu. */
    async show(p) {
      await editor.refreshLibrary();
      const filename = soundName(str(p, "filename") ?? "");
      await editor.refreshLibrary([filename]);
      const entry = editor.library?.sounds.find((s) => s.name === filename);
      if (!entry?.editable) return { opened: false, reason: "pas de recette pour ce son" };
      if (editor.doc.dirty) {
        notify(`a rendu ${filename}`);
        return { opened: false, reason: "le son ouvert a des modifications non enregistrées : il reste ouvert" };
      }
      if (filename === editor.doc.name) {
        await commands.reloadFromDisk(`Rendu de ${filename}`, "claude");
        notify(`a rendu ${filename}`);
        return { opened: true, reason: "nouvelle version du son ouvert, annulable avec ⌘Z" };
      }
      await commands.openSound(filename, false);
      notify(`a rendu ${filename}`);
      return { opened: true };
    },
  };
}

const round = (v: number) => Math.round(v * 100) / 100;
