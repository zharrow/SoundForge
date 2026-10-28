# sound-forge

Moteur de synthèse d'effets sonores pour jeux vidéo, en Rust, avec un serveur MCP pour Claude Code
et une app d'édition visuelle, Sound Forge, que Claude pilote en direct (comme Pixel Forge).
Il produit des WAV mono 44,1 kHz prêts pour Godot.

```
sound-forge/
├── src/        moteur (aucune dépendance) + CLI `sound-forge`
├── mcp/        serveur MCP `sound-forge-mcp` (rmcp)
└── app/        l'app Sound Forge (Tauri 2 : interface TypeScript, coquille Rust qui embarque le moteur)
```

## L'app Sound Forge

On y retouche un son à la main et on l'entend à chaque geste. Deux modes, au choix en haut à droite.

**Mode simple** (par défaut) : on agit par ce qu'on entend, sans connaître la synthèse.
- **Réglages en mots** : six curseurs pour tout le son (plus grave / plus aigu, plus court / plus long,
  plus sombre / plus clair, plus doux / plus agressif, plus sec / plus lointain, plus plein / plus soufflé).
  On tire vers le mot, le son suit, le curseur revient au centre : on peut pousser plusieurs fois.
- **Explorer en écoutant** : six versions voisines du son ; on les écoute, on garde la meilleure, six
  autres naissent autour d'elle. L'audace règle l'écart.
- **Couches à dessiner** : chaque couche montre son rôle (attaque, corps, queue, souffle, voix ; donné ou
  deviné) expliqué en une phrase, son enveloppe et sa hauteur sous forme de courbes dont on tire les points.
  « Tous les réglages » ouvre le reste.

**Mode détaillé** : tous les réglages de chaque couche (source, hauteur, enveloppe, filtre, résonateur,
unisson, vibrato), avec les mêmes courbes et une phrase d'explication par bloc.

Partout :
- **Avant / Comparer** : la version enregistrée (ou celle de l'ouverture), puis la tienne, pour entendre ce
  que les retouches ont changé.
- **Enregistrer ma voix** (⌘R) : 10 s au plus ; le silence autour est retiré, deux poignées ajustent le début
  et la fin. La prise va dans `voix/<nom>.wav` et devient la source d'une couche (`sample`) : transposée
  (plus grave = plus lent et plus gros), à l'envers, filtrée, frappée comme un objet, en variantes. Claude
  l'écoute par son nom (`analyze_sound`, `spectrogram` « voix/nom ») et peut s'en servir dans ses sons.
- **Bibliothèque** : les sons du dossier (ceux qui ont une recette sont retouchables), les voix, les presets.
  Ce que Claude rend apparaît aussitôt.
- **Aperçu** : forme d'onde et tête de lecture, une barre par couche sur l'axe du temps, le spectrogramme du
  moteur (le même que voit Claude), durée, crête, RMS et énergie par bande. S isole une couche, M la coupe.
- **Écoute** : Espace, 1 à 9 pour une variante, « Boucle », « Auto » (entendre chaque retouche). La variante
  entendue est, octet pour octet, le fichier qui sera écrit.
- **Enregistrer** (⌘S) écrit les WAV et la recette exactement comme `render_sound`.

Les retouches de Claude entrent dans le même historique (⌘Z) avec son nom. Claude ne remplace jamais un son
qui a des modifications non enregistrées.

```bash
cd app
npm install
npm run install-app  # compile et installe ~/Applications/Sound Forge.app
npm test             # historique, modèle, pont avec Claude
npx tauri dev        # développement (port 1421)
npm run banc         # banc d'essai dans un navigateur, sans Tauri : http://localhost:1499/dev/index.html
```

Le banc d'essai (`dev/`) sert l'interface avec un faux côté natif : il permet de vérifier la mise en page et
les gestes quand on ne voit pas l'app. Le pont répond aussi à `diagnostics` (taille des zones, dernières
erreurs) : une fenêtre cachée n'est pas dessinée par macOS, c'est le moyen de savoir ce que la page affiche.

L'app lit et écrit le même dossier que le serveur MCP : `SOUND_OUTPUT_DIR`, sinon `~/GameAssets/sfx`. Le
pont local écoute sur 127.0.0.1, avec un jeton écrit dans `~/.sound-forge/app.json`.

## Musique : de vrais instruments

`render_music` joue une partition avec des instruments échantillonnés et rend des **stems stéréo
synchronisés** (même longueur à l'échantillon près, un seul gain pour tous, jamais normalisés séparément).
En boucle, ce qui résonne après la dernière mesure revient sur le début : la couture ne s'entend pas.

- **Partition** (JSON) : tempo, mesure, nombre de mesures, pistes. Une piste a un instrument, un stem
  (« ambience », « tension », « combat », « melody »…), un volume, un placement gauche-droite, une part de
  réverbération commune, et des notes (`at` et `len` en temps, `note` « D4 » ou MIDI, `vel` 0..1). Les motifs
  (`patterns`, joués par `play`) évitent de réécrire un leitmotiv.
- **Instruments** : ceux du catalogue (`list_instruments`), ou un son de Sound Forge (`{"synth": son}`) joué
  à la hauteur de chaque note, pour les nappes et les textures.
- **Calibrage** : une note jouée à fond sort au même niveau (-20 dB RMS) quel que soit l'instrument ; les
  volumes d'une partition se comparent donc d'un instrument à l'autre.
- Sortie : `musique/<nom>_<stem>.wav`, `musique/<nom>_mix.wav` et la recette `musique/<nom>.json`.

### La bibliothèque d'instruments

Dossier `~/GameAssets/instruments/` (ou `SOUND_INSTRUMENTS_DIR`) :
- `VSCO-2-CE/` : une sélection de [VSCO 2 Community Edition](https://github.com/sgossner/VSCO-2-CE)
  (Versilian Studios, **CC0**, aucune obligation de citation) : cordes, bois, cuivres, piano, harpe,
  percussions accordées et d'orchestre, cloches népalaises (1,1 Go) ;
- `selection.json` : la liste des instruments à en tirer, écrite à la main (dossier ou kit de percussions,
  jeu tenu ou frappé, extinction, correction de volume) ;
- `catalogue.json` : construit par `build_instruments`. La note, la nuance et la prise se lisent dans le nom
  des fichiers ; la hauteur réelle de chaque fichier est mesurée, et l'octave est corrigé dossier par dossier
  (les fabricants ne numérotent pas tous les octaves de la même façon).

Pour ajouter un instrument : déposer ses enregistrements, l'ajouter à `selection.json`, relancer
`build_instruments`.

### L'onglet Musique de l'app

Les morceaux de `musique/` s'ouvrent dans l'onglet **Musique** de Sound Forge (bascule en haut à gauche).
- **Écouter par stems** : chaque stem se coupe ou revient pendant la lecture, sans décrochage ; les boutons
  Exploration, Danger, Combat font entendre ce que le jeu ferait selon la situation.
- **Pistes** : en miniature sur toute la longueur ; S écoute une piste seule, M la coupe. À droite : instrument
  (du catalogue, ou un son de l'onglet Sons joué comme instrument), stem, volume, placement, espace,
  transposition, jeu humain ; dupliquer, supprimer, ajouter.
- **Piano roll** de la piste choisie : clic pour poser une note (on l'entend ; glisser pour la rallonger),
  glisser pour la déplacer, bord droit pour la rallonger, double-clic ou Suppr pour l'enlever, flèches pour la
  bouger. Grille au quart, à la moitié ou au temps ; les autres pistes en filigrane. Les notes jouées par un
  motif sont hachurées : « Détacher les motifs » les rend modifiables une à une.
- **Morceau** : tempo, mesures, temps par mesure, espace commun, niveau, graine, boucle ou jingle.
- Seules les pistes retouchées sont refaites (le reste est gardé en mémoire) ; ⌘Z annule tout, les retouches
  de Claude comprises. **Enregistrer** écrit les stems et la recette exactement comme `render_music`, et un
  morceau rendu par Claude s'ouvre tout seul dans l'onglet.

## Installation

```bash
cargo install --path .        # CLI
cargo install --path mcp      # serveur MCP
```

## Brancher le serveur sur Claude Code

```bash
claude mcp add sound-forge \
  -e SOUND_OUTPUT_DIR="$HOME/GameAssets/sfx" \
  -- "$HOME/.cargo/bin/sound-forge-mcp"
```

Vérifier avec `claude mcp list`, puis dans Claude Code :

> Fais-moi un son de coup d'épée plus lourd que le preset, avec 4 variantes, et vérifie le spectrogramme.

### Outils exposés

| Outil | Rôle |
|---|---|
| `list_presets` | Liste les 20 presets |
| `get_preset` | JSON complet d'un preset, à modifier |
| `render_preset` | Rend un preset (avec variantes) |
| `render_sound` | Rend un son décrit en JSON |
| `list_files` | WAV et recettes du dossier de travail |
| `get_recipe` | La recette d'un son déjà rendu, pour le retoucher |
| `render_recipe` | Refait un son à l'identique depuis sa recette |
| `analyze_sound` | Durée, crête, RMS, énergie sur 7 bandes |
| `spectrogram` | Image forme d'onde + spectrogramme, pour que Claude « voie » le son |
| `compare_sounds` | Détecte le masquage entre deux sons (SFX contre musique) |
| `open_app` | Ouvre l'app Sound Forge au premier plan |
| `get_document` | Le son ouvert dans l'app : état, historique récent, prise complète en JSON |
| `open_sound` | Ouvre un son du dossier ou un preset dans l'app (refusé si le son affiché n'est pas enregistré, sauf `force`) |
| `new_sound` | Nouveau son dans l'app, vide ou depuis un JSON |
| `edit_sound` | Retouche le son ouvert : étape annulable à ton nom, entendue aussitôt ; renvoie mesures et image |
| `play_sound` | Fait entendre le son ouvert (ou une variante) |
| `save_sound` | Enregistre WAV et recette depuis l'app |

Après `render_sound`, `render_preset` ou `render_recipe`, l'app affiche le son si elle est ouverte.

Le serveur lit et écrit uniquement dans `SOUND_OUTPUT_DIR`. Pour analyser un morceau de référence
ou une piste de ta musique, copie le WAV dans ce dossier.

## CLI

```bash
sound-forge list
sound-forge render all --out assets/sfx
sound-forge render sword_swing --variants 4 --amount 0.4 --out assets/sfx
sound-forge analyze assets/sfx/impact.wav
sound-forge compare assets/sfx/sword_swing.wav music/combat.wav
sound-forge spectrogram assets/sfx/riser.wav riser.png
```

`analyze` et `compare` lisent les WAV PCM 16/24 bits et flottants 32 bits, mono ou stéréo.

## Presets

- **Gameplay** : `jump`, `dash`, `sword_swing`, `impact`, `pickup`, `powerup`, `explosion`, `death`
- **Interface** : `ui_click`, `ui_confirm`
- **Transitions musicales** : `riser` (pic à 2 s, soit 1 mesure à 120 BPM), `downlifter`,
  `stinger_victory`, `stinger_boss`
- **Objets (synthèse modale)** : `wood_knock`, `stone_hit`, `metal_clang`, `bell`, `glass_tap`,
  `bone_knock`

## Recettes

Chaque rendu par le serveur MCP écrit sa **recette** à côté des WAV : `cloche.json` pour
`cloche.wav`, ou pour `cloche_01.wav`, `cloche_02.wav`… Elle contient le son, la graine, les
réglages de variantes et la version du moteur. Même binaire + même recette = mêmes fichiers,
octet pour octet. Une compilation de débogage peut arrondir autrement quelques échantillons
(un bit de poids faible) : c'est inaudible, mais une comparaison octet par octet le voit.

- retoucher un son : `get_recipe` → modifier `sound` → `render_sound` ;
- le refaire après une mise à jour du moteur : `render_recipe` (il signale si la recette vient
  d'une autre version, le son pouvant alors légèrement différer).

Les recettes passent par serde : elles n'existent qu'avec la fonctionnalité `schema`, celle du
serveur MCP. La CLI, sans dépendance, ne les écrit pas.

## Format JSON d'un son

C'est le format de `get_preset` et `render_sound` :

```json
{
  "name": "laser",
  "layers": [
    {
      "waveform": "saw",
      "pitch": { "start": 1800, "end": 200, "time": 0.25 },
      "envelope": { "attack": 0.002, "decay": 0.3 },
      "filter": {
        "mode": "band_pass",
        "cutoff": { "start": 4000, "end": 600, "time": 0.25 },
        "resonance": 0.6
      },
      "unison": { "voices": 3, "detune_cents": 20 }
    }
  ],
  "effects": {
    "echo": { "time": 0.08, "feedback": 0.35, "mix": 0.3 },
    "reverb": { "room": 0.4, "damping": 0.4, "mix": 0.2 }
  },
  "peak_db": -3
}
```

- `waveform` : `sine`, `triangle`, `saw`, `{"square": {"duty": 0.5}}`, `white_noise`, `brown_noise`
- `waveform` `{"sample": {"file": "voix/souffle", "reverse": false}}` : un enregistrement du dossier de travail ;
  la hauteur règle sa vitesse (440 Hz = vitesse d'origine, 880 = une octave plus haut)
- `name` (facultatif) : rôle de la couche (« attaque », « corps », « queue »…), sans effet sur le son
- `filter.mode` : `low_pass`, `high_pass`, `band_pass`
- Champs facultatifs d'une couche : `filter`, `vibrato`, `unison`, `gain` (1), `start` (0)
- Champ facultatif `resonator` : voir plus bas
- Limites : 16 couches, 10 s (queue des résonateurs comprise), 16 voix d'unisson par couche

## Synthèse modale : le coup et l'objet

Un bois, une pierre ou une cloche ne se distinguent pas par la forme d'onde du coup, mais par
les fréquences auxquelles l'objet vibre ensuite et la vitesse à laquelle chacune s'éteint.
Ajouter un `resonator` à une couche en fait le **coup** ; le résonateur est l'**objet**.
La hauteur de la couche (`pitch.start`) donne la fondamentale de l'objet.

```json
{
  "waveform": "white_noise",
  "pitch": { "start": 420, "end": 420, "time": 0 },
  "envelope": { "attack": 0.0005, "decay": 0.006 },
  "filter": { "mode": "low_pass", "cutoff": { "start": 3500, "end": 3500, "time": 0 }, "resonance": 0.2 },
  "resonator": { "material": "wood", "decay": 0.6, "brightness": 0.3, "dry": 0.1 }
}
```

- `material` : `wood`, `stone`, `metal`, `bell`, `glass`, `bone`, ou
  `{"custom": {"modes": [{"ratio": 1, "decay": 0.5, "gain": 1}, …]}}` (32 modes au plus)
- `decay` (1) : multiplie les durées de résonance. 0.5 = objet étouffé, 2 = objet qui chante
- `brightness` (0.5) : 0 = sourd, 1 = brillant. Agit sur le gain ET la durée des modes aigus
- `dry` (0) : part du coup qui reste audible tel quel

Le coup règle le geste : une rafale de bruit blanc de quelques millisecondes est un maillet dur,
plus longue et filtrée c'est un maillet doux, du bruit brun tenu c'est un frottement, une note
tenue fait chanter l'objet.

## Dans Godot 4

1. Copier les `.wav` dans le projet, Godot les importe automatiquement.
2. Variantes : créer un **AudioStreamRandomizer**, y ajouter `sword_swing_01.wav` à `_04.wav`,
   l'assigner au `stream` de l'`AudioStreamPlayer2D`.
3. Garder le WAV pour les SFX. La musique passera en Ogg Vorbis.
4. Mixage : un bus `Music` et un bus `SFX`, avec un compresseur en sidechain sur `Music`
   déclenché par `SFX`, pour que la musique s'efface pendant les actions.

## Tests

```bash
cargo test -p sound-forge
```
