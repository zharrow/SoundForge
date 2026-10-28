/**
 * Le son ouvert et son historique. Chaque retouche est une étape annulable (⌘Z), qu'elle vienne de
 * l'utilisateur ou de Claude : les deux partagent le même historique, et l'étape garde son auteur.
 *
 * Une étape mémorise l'état complet de la prise (quelques kilo-octets de JSON) : annuler, c'est
 * relire l'étape précédente. Un geste continu (glisser un curseur) ne fait qu'une étape :
 * `begin` au début, des modifications libres, `commit` à la fin.
 */

import { type Take, clone } from "./model";

export type Author = "user" | "claude";

export interface Step {
  label: string;
  author: Author;
  snapshot: string;
}

const MAX_STEPS = 200;

/** `T` : ce qu'on édite (la prise d'un son, ou un morceau et sa graine). */
export class SoundDoc<T extends object = Take> {
  take: T;
  /** Nom de base des fichiers dans le dossier de travail ; null tant que le son n'a jamais été enregistré. */
  name: string | null;
  /** Preset d'origine, gardé dans la recette. */
  preset: string | null;
  steps: Step[];
  index = 0;
  /** État de référence : la version enregistrée, ou l'état d'ouverture d'un son jamais enregistré. */
  private saved: string;
  private pending: { label: string; author: Author; before: string } | null = null;

  /** Un son ouvert (du disque, d'un preset ou neuf) n'est « modifié » qu'après une retouche. */
  constructor(take: T, name: string | null, preset: string | null, label = "Ouverture") {
    this.take = clone(take);
    this.name = name;
    this.preset = preset;
    const snapshot = JSON.stringify(this.take);
    this.steps = [{ label, author: "user", snapshot }];
    this.saved = snapshot;
  }

  get dirty(): boolean {
    return this.saved !== JSON.stringify(this.take);
  }

  /** La version de référence pour « avant / après » : l'enregistrée, ou celle de l'ouverture. */
  get reference(): T {
    return JSON.parse(this.saved) as T;
  }

  get canUndo(): boolean {
    return this.index > 0;
  }

  get canRedo(): boolean {
    return this.index < this.steps.length - 1;
  }

  /** Début d'un geste : les modifications qui suivent formeront une seule étape. */
  begin(label: string, author: Author = "user"): void {
    if (this.pending) this.commit();
    this.pending = { label, author, before: JSON.stringify(this.take) };
  }

  /** Fin du geste : une étape s'ajoute si la prise a changé. Rend vrai dans ce cas. */
  commit(): boolean {
    const pending = this.pending;
    this.pending = null;
    if (!pending) return false;
    const snapshot = JSON.stringify(this.take);
    if (snapshot === pending.before) return false;
    this.steps.splice(this.index + 1);
    this.steps.push({ label: pending.label, author: pending.author, snapshot });
    if (this.steps.length > MAX_STEPS) this.steps.splice(1, this.steps.length - MAX_STEPS);
    this.index = this.steps.length - 1;
    return true;
  }

  /** Renomme l'étape en cours (un geste dont on ne connaît l'effet qu'à la fin). */
  relabel(label: string): void {
    if (this.pending) this.pending.label = label;
  }

  get editing(): boolean {
    return this.pending !== null;
  }

  /** Une retouche en une fois. */
  edit(label: string, change: (take: T) => void, author: Author = "user"): boolean {
    this.begin(label, author);
    change(this.take);
    return this.commit();
  }

  undo(): boolean {
    this.commit();
    if (!this.canUndo) return false;
    this.goto(this.index - 1);
    return true;
  }

  redo(): boolean {
    this.commit();
    if (!this.canRedo) return false;
    this.goto(this.index + 1);
    return true;
  }

  /** Revient à une étape de l'historique (clic dans le panneau). */
  goto(index: number): void {
    this.commit();
    this.index = Math.max(0, Math.min(index, this.steps.length - 1));
    this.take = JSON.parse(this.steps[this.index].snapshot) as T;
  }

  markSaved(name: string): void {
    this.commit();
    this.name = name;
    this.saved = JSON.stringify(this.take);
  }

  /** Revient à la version enregistrée, comme une étape annulable. */
  revert(): boolean {
    if (!this.dirty) return false;
    const saved = this.saved;
    return this.edit("Retour à la version enregistrée", (take) => Object.assign(take, JSON.parse(saved)));
  }
}
