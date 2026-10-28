/**
 * Les commandes de l'éditeur, partagées par les menus, les boutons, le clavier et le pont avec Claude.
 * Ouvrir un son quand le son en cours a des modifications demande confirmation (sauf pour Claude, que le
 * pont arrête avant : il ne jette jamais le travail de l'utilisateur).
 */

import { type Author, SoundDoc } from "./doc";
import type { Editor } from "./editor";
import { normalizeSound, normalizeTake, newTake, sameSound, sampleLayer, validName } from "./model";
import * as native from "./native";
import type { StatusBar } from "./ui/panels";
import { addLayer, deleteLayer } from "./ui/layers";
import { openRecorder } from "./ui/recorder";

export interface Commands {
  [command: string]: ((...args: never[]) => unknown) | undefined;
  play(): void;
  "new"(): Promise<void>;
  save(): Promise<void>;
  save_as(): Promise<void>;
  revert(): void;
  reveal(): Promise<void>;
  reveal_folder(): Promise<void>;
  undo(): void;
  redo(): void;
  add_layer(): void;
  duplicate_layer(): void;
  delete_layer(): void;
  prev_variant(): void;
  next_variant(): void;
  new_seed(): void;
  shortcuts(): void;
  openSound(name: string, ask?: boolean): Promise<boolean>;
  openPreset(name: string, ask?: boolean): Promise<boolean>;
  reloadFromDisk(label: string, author: Author): Promise<boolean>;
  saveAs(name: string, overwrite?: boolean): Promise<string[]>;
  before(): Promise<void>;
  compare(): Promise<void>;
  record(): void;
  addRecording(file: string): void;
  playRecording(file: string): Promise<void>;
}

export function createCommands(editor: Editor, status: StatusBar, askName: (initial: string) => Promise<string | null>): Commands {
  const discardOk = async (): Promise<boolean> =>
    !editor.doc.dirty ||
    native.confirm(`« ${editor.doc.name ?? "Son sans nom"} » a des modifications non enregistrées. Les abandonner ?`, "Sound Forge", "Abandonner");

  const lastLayer = () => editor.doc.take.sound.layers.length - 1;

  const commands: Commands = {
    play: () => editor.player.toggle(),

    async new() {
      if (!(await discardOk())) return;
      editor.open(new SoundDoc(newTake(), null, null, "Nouveau son"));
    },

    async openSound(name, ask = true) {
      if (ask && name !== editor.doc.name && !(await discardOk())) return false;
      const opened = await native.loadRecipe(name);
      const take = normalizeTake({ sound: opened.sound, variants: opened.variants, amount: opened.amount, seed: opened.seed });
      editor.open(new SoundDoc(take, opened.name, opened.preset, `Ouverture de ${opened.name}`));
      return true;
    },

    /** Le son ouvert a été réécrit sur le disque : la nouvelle version devient une étape annulable. */
    async reloadFromDisk(label, author) {
      const name = editor.doc.name;
      if (!name) return false;
      const opened = await native.loadRecipe(name);
      const disk = normalizeTake({ sound: opened.sound, variants: opened.variants, amount: opened.amount, seed: opened.seed });
      if (editor.doc.name !== name || sameSound(disk, editor.doc.take)) return false;
      editor.edit(label, (take) => Object.assign(take, disk), author);
      editor.doc.preset = opened.preset;
      editor.doc.markSaved(name);
      editor.emit("doc");
      return true;
    },

    async openPreset(name, ask = true) {
      if (ask && !(await discardOk())) return false;
      const sound = normalizeSound(await native.preset(name));
      editor.open(new SoundDoc(newTake(sound), null, name, `Preset ${name}`));
      return true;
    },

    async saveAs(name, overwrite = false) {
      if (!validName(name)) throw new Error(`Nom invalide « ${name} » : lettres, chiffres, _ et - uniquement (64 au plus).`);
      if (!overwrite && name !== editor.doc.name && (await native.exists(name))) {
        throw new Error(`« ${name} » existe déjà dans le dossier.`);
      }
      editor.doc.commit();
      const take = editor.doc.take;
      take.sound.name = name;
      editor.savedAt = Date.now();
      const { files } = await native.save(name, take, editor.doc.preset);
      editor.savedAt = Date.now();
      editor.doc.markSaved(name);
      editor.emit("doc");
      status.show(`Enregistré : ${files.join(", ")}`);
      await editor.refreshLibrary();
      return files;
    },

    async save() {
      if (!editor.doc.name) return commands.save_as();
      await commands.saveAs(editor.doc.name, true);
    },

    async save_as() {
      const name = await askName(editor.doc.name ?? editor.doc.preset ?? "nouveau_son");
      if (!name) return;
      const exists = name !== editor.doc.name && (await native.exists(name));
      if (exists && !(await native.confirm(`« ${name} » existe déjà. Le remplacer ?`, "Enregistrer sous", "Remplacer"))) return;
      await commands.saveAs(name, true);
    },

    revert() {
      if (editor.doc.revert()) {
        editor.emit("doc");
        editor.requestRender(editor.autoplay);
      }
    },

    before: async () => void (await editor.playBefore()),
    compare: () => editor.compare(),
    record: () => openRecorder(editor),

    addRecording(file) {
      const seconds = editor.library?.recordings.find(([name]) => name === file)?.[1] ?? 1;
      editor.edit(`Voix « ${file.replace(/^voix\//, "")} » ajoutée`, (take) => void take.sound.layers.push(sampleLayer(file, seconds)));
    },

    async playRecording(file) {
      editor.player.playBuffer(await editor.player.decode(await native.recordingWav(file)));
    },

    reveal: () => native.reveal(editor.doc.name),
    reveal_folder: () => native.reveal(null),
    undo: () => editor.undo(),
    redo: () => editor.redo(),
    add_layer: () => addLayer(editor),
    duplicate_layer: () => addLayer(editor, lastLayer()),
    delete_layer: () => deleteLayer(editor, lastLayer()),
    prev_variant: () => editor.setVariant(editor.variant - 1),
    next_variant: () => editor.setVariant(editor.variant + 1),
    new_seed: () => editor.edit("Nouvelle graine", (take) => (take.seed = Math.floor(Math.random() * 10000))),

    shortcuts() {
      void native.confirm(
        [
          "Espace : écouter / arrêter",
          "1 à 9 : écouter une variante ; ⌘← ⌘→ : variante précédente, suivante",
          "⌘Z, ⇧⌘Z : annuler, rétablir (les retouches de Claude aussi)",
          "⌘S, ⇧⌘S : enregistrer, enregistrer sous",
          "⌘N : nouveau son ; ⇧⌘N : nouvelle couche ; ⌘D : dupliquer la dernière couche",
          "⌘G : nouvelle graine (d'autres variantes)",
          "⌘R : enregistrer ta voix",
          "",
          "Les dessins (enveloppe, hauteur, filtre) se modifient en tirant leurs points.",
          "Avant / Comparer : la version enregistrée, puis la tienne.",
          "",
          "Réglages : glisser le curseur, saisir un nombre (Entrée), ↑ ↓ dans le nombre,",
          "ou glisser le nom du réglage (⇧ : plus fin). Double-clic sur le nom : valeur par défaut.",
        ].join("\n"),
        "Raccourcis",
        "OK",
      );
    },
  };
  return commands;
}
