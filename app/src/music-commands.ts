/**
 * Commandes de l'onglet Musique : ouvrir, créer, enregistrer, et reprendre une version écrite par Claude.
 * Ouvrir un morceau quand le morceau en cours a des modifications demande confirmation.
 */

import type { Author } from "./doc";
import { sameSound, validName } from "./model";
import type { MusicEditor } from "./music-editor";
import { newMusic, normalizeMusic } from "./music-model";
import * as native from "./native";
import type { MusicCommands } from "./ui/music";
import type { StatusBar } from "./ui/panels";

export interface MusicCommandSet extends MusicCommands {
  reloadFromDisk(label: string, author: Author): Promise<boolean>;
  saveAs(name: string, overwrite?: boolean): Promise<string[]>;
}

export function createMusicCommands(editor: MusicEditor, status: StatusBar, askName: (initial: string) => Promise<string | null>): MusicCommandSet {
  const discardOk = async (): Promise<boolean> =>
    !editor.doc.dirty ||
    native.confirm(`« ${editor.doc.name ?? "Morceau sans nom"} » a des modifications non enregistrées. Les abandonner ?`, "Sound Forge", "Abandonner");

  const commands: MusicCommandSet = {
    async openPiece(name, ask = true) {
      if (ask && name !== editor.doc.name && !(await discardOk())) return false;
      const recipe = await native.loadMusic(name);
      editor.open({ music: normalizeMusic(recipe.music), seed: recipe.seed }, name, `Ouverture de ${name}`);
      return true;
    },

    async newPiece() {
      if (!(await discardOk())) return;
      editor.open({ music: newMusic(), seed: 1 }, null, "Nouveau morceau");
    },

    /** Le morceau ouvert a été réécrit sur le disque (par Claude) : la nouvelle version devient une étape annulable. */
    async reloadFromDisk(label, author) {
      const name = editor.doc.name;
      if (!name) return false;
      const recipe = await native.loadMusic(name);
      const disk = { music: normalizeMusic(recipe.music), seed: recipe.seed };
      if (editor.doc.name !== name || sameSound(disk, editor.doc.take)) return false;
      editor.edit(label, (take) => Object.assign(take, disk), author);
      editor.doc.markSaved(name);
      editor.emit("doc");
      return true;
    },

    async saveAs(name, overwrite = false) {
      if (!validName(name)) throw new Error(`Nom invalide « ${name} » : lettres, chiffres, _ et - uniquement.`);
      if (!overwrite && name !== editor.doc.name && (await native.musicExists(name))) throw new Error(`« ${name} » existe déjà.`);
      editor.doc.commit();
      editor.music.name = name;
      editor.savedAt = Date.now();
      const files = await native.saveMusic(name, editor.music, editor.doc.take.seed);
      editor.savedAt = Date.now();
      editor.doc.markSaved(name);
      editor.emit("doc");
      status.show(`Enregistré dans musique/ : ${files.join(", ")}`);
      await editor.refreshLibrary();
      return files;
    },

    async save() {
      if (!editor.doc.name) return commands.save_as();
      await commands.saveAs(editor.doc.name, true);
    },

    async save_as() {
      const name = await askName(editor.doc.name ?? "mus_nouveau");
      if (!name) return;
      const exists = name !== editor.doc.name && (await native.musicExists(name));
      if (exists && !(await native.confirm(`« ${name} » existe déjà. Le remplacer ?`, "Enregistrer sous", "Remplacer"))) return;
      await commands.saveAs(name, true);
    },

    undo: () => editor.undo(),
    redo: () => editor.redo(),
    play: () => editor.player.toggle(),
    new: () => commands.newPiece(),
  };
  return commands;
}
