/**
 * Les mots de l'éditeur : rôles des couches et explication de chaque réglage, en une phrase, par ce qu'on
 * entend plutôt que par ce que fait le calcul. C'est la grammaire de la direction audio d'Eku :
 * une attaque (le choc), un corps (la matière), une queue (l'espace).
 */

import { type Layer, envelopeDuration, isNoise, sampleOf } from "../model";

export const ROLES: [string, string][] = [
  ["Attaque", "Le choc des premières millisecondes : ce qui rend le son net et lisible."],
  ["Corps", "La matière principale, ce qu'on retient du son : sa note, son grain."],
  ["Queue", "Ce qui reste après : la résonance, l'espace, l'extinction."],
  ["Souffle", "Un bruit d'air ou de matière, sans note : il donne la texture."],
  ["Voix", "Un enregistrement : ta voix ou un bruit fait à la bouche, transformé."],
];

export function roleText(role: string): string {
  return ROLES.find(([name]) => name.toLowerCase() === role.toLowerCase())?.[1] ?? "";
}

/** Le rôle d'une couche : celui qu'on lui a donné, sinon une estimation d'après sa forme. */
export function roleOf(layer: Layer): { role: string; guessed: boolean } {
  if (layer.name) return { role: layer.name, guessed: false };
  return { role: guessRole(layer), guessed: true };
}

export function guessRole(layer: Layer): string {
  if (sampleOf(layer.waveform)) return "Voix";
  const length = envelopeDuration(layer.envelope);
  if (length <= 0.06 && layer.start < 0.02) return "Attaque";
  if (layer.start >= 0.05 || (layer.resonator && layer.resonator.decay >= 1.5)) return "Queue";
  if (isNoise(layer.waveform) && !layer.resonator) return "Souffle";
  return "Corps";
}

/** Une phrase par bloc de réglages. */
export const BLOCK_HELP: Record<string, string> = {
  Source: "Ce qui vibre : une onde (du son pur au son riche), un bruit (souffle), ou un enregistrement.",
  Hauteur: "Grave ou aigu, et s'il glisse : une montée donne de l'élan, une descente une chute.",
  "Fondamentale de l'objet": "La note de l'objet frappé : plus bas, il paraît plus gros.",
  Transposition: "Plus grave = plus lent et plus gros ; plus aigu = plus rapide et plus petit.",
  Enveloppe: "Comment le volume naît et meurt. Tire les points du dessin.",
  Filtre: "Retire une partie du son : les aigus (plus sourd, plus loin), les graves (plus fin), ou garde une bande.",
  Résonateur: "Fait de la couche un coup porté à un objet : bois, pierre, métal, cloche, verre, os.",
  Unisson: "Plusieurs copies légèrement désaccordées : plus large, plus épais.",
  Vibrato: "La hauteur qui tremble : vivant, inquiétant ou comique selon la vitesse.",
  Niveau: "Le volume final du fichier, pour équilibrer les sons entre eux.",
  Saturation: "Salit le son : plus de mordant, plus d'agressivité.",
  Écho: "Des répétitions du son, de plus en plus faibles.",
  Réverbération: "La pièce autour du son : un placard, une grotte, une cathédrale.",
  Variantes: "Plusieurs versions légèrement différentes, pour qu'un son répété ne lasse pas l'oreille.",
};
