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
| `list_presets` | Liste les 14 presets |
| `get_preset` | JSON complet d'un preset, à modifier |
| `render_preset` | Rend un preset (avec variantes) |
| `render_sound` | Rend un son décrit en JSON |
| `list_files` | WAV du dossier de travail |
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
- Limites : 16 couches, 10 s, 16 voix d'unisson par couche

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
