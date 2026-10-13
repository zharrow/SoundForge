# sound-forge

Moteur de synthèse d'effets sonores pour jeux vidéo, en Rust, avec un serveur MCP pour Claude Code.
Il produit des WAV mono 44,1 kHz prêts pour Godot.

```
sound-forge/
├── src/        moteur (aucune dépendance) + CLI `sound-forge`
└── mcp/        serveur MCP `sound-forge-mcp` (rmcp)
```

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
