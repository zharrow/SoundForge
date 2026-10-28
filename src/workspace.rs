//! Dossier de travail partagé par la CLI et le serveur MCP.
//! Tous les fichiers lus ou écrits restent confinés dans ce dossier.

use std::path::{Path, PathBuf};

use crate::bands::Overlap;
use crate::mutate::vary;
use crate::render::{SAMPLE_RATE, render};
use crate::report::{self, Report};
use crate::oscillator::Waveform;
use crate::rng::Rng;
use crate::sample::{self, RECORDINGS_DIR, SampleData};
use crate::sound::Sound;
use crate::wav::write_wav;

const MAX_NAME_LENGTH: usize = 64;
const MAX_VARIANTS: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportOptions {
    /// 1 = un seul fichier ; plus = variantes numérotées `nom_01.wav`, `nom_02.wav`…
    pub variants: u32,
    /// Écart entre variantes, de 0 (identiques) à 1 (très différentes).
    pub amount: f32,
    /// Même graine = même résultat.
    pub seed: u64,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            variants: 1,
            amount: 0.4,
            seed: 1,
        }
    }
}

/// Une version à rendre : le son (varié ou non), sa graine et le suffixe de son fichier.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub sound: Sound,
    pub seed: u64,
    /// `""` pour un fichier unique, `"_01"`, `"_02"`… pour des variantes.
    pub suffix: String,
}

/// Les versions qu'`export` écrit, dans l'ordre, avec leurs graines. L'éditeur les écoute par la même
/// fonction : la variante 02 entendue dans l'app est, octet pour octet, le fichier `nom_02.wav`.
pub fn variants(sound: &Sound, options: &ExportOptions) -> Vec<Variant> {
    if options.variants <= 1 {
        return vec![Variant {
            sound: sound.clone(),
            seed: options.seed,
            suffix: String::new(),
        }];
    }
    let mut rng = Rng::new(options.seed);
    (1..=options.variants.min(MAX_VARIANTS))
        .map(|index| Variant {
            sound: vary(sound, options.amount, &mut rng),
            seed: options.seed + u64::from(index),
            suffix: format!("_{index:02}"),
        })
        .collect()
}

/// Rend une version de `variants`, telle qu'`export` l'écrirait.
pub fn render_variant(variant: &Variant) -> Vec<f32> {
    render(&variant.sound, variant.seed)
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Charge les enregistrements dont le son se sert (couches `sample`). À faire avant le rendu : la
    /// recette ne contient que leurs noms.
    pub fn resolve(&self, sound: &Sound) -> Result<Sound, String> {
        let mut sound = sound.clone();
        for layer in &mut sound.layers {
            if let Waveform::Sample(source) = &mut layer.waveform {
                let path = self.recording_path(&source.file)?;
                source.data = SampleData(Some(sample::load(&path)?));
            }
        }
        Ok(sound)
    }

    /// Chemin d'un enregistrement : « voix/souffle » -> <dossier>/voix/souffle.wav (jamais hors du dossier).
    pub fn recording_path(&self, file: &str) -> Result<PathBuf, String> {
        let file = file.strip_suffix(".wav").unwrap_or(file);
        let parts: Vec<&str> = file.split('/').collect();
        if parts.len() > 2 {
            return Err(format!("Chemin d'enregistrement invalide « {file} »."));
        }
        for part in &parts {
            validate_name(part)?;
        }
        Ok(self.root.join(format!("{file}.wav")))
    }

    /// Écrit un enregistrement (voix…) dans le sous-dossier `voix/`. Renvoie son nom pour les recettes.
    pub fn write_recording(&self, name: &str, samples: &[f32], sample_rate: u32) -> Result<String, String> {
        validate_name(name)?;
        let dir = self.root.join(RECORDINGS_DIR);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
        let path = dir.join(format!("{name}.wav"));
        write_wav(&path, samples, sample_rate).map_err(|e| format!("{} : {e}", path.display()))?;
        Ok(format!("{RECORDINGS_DIR}/{name}"))
    }

    /// Noms des enregistrements (« voix/souffle »), triés.
    pub fn list_recordings(&self) -> Vec<String> {
        Workspace::new(self.root.join(RECORDINGS_DIR))
            .list_wavs()
            .into_iter()
            .map(|name| format!("{RECORDINGS_DIR}/{name}"))
            .collect()
    }

    /// Rend le son et l'écrit en WAV (ou ses variantes). Renvoie un rapport par fichier.
    pub fn export(
        &self,
        sound: &Sound,
        filename: &str,
        options: &ExportOptions,
    ) -> Result<Vec<Report>, String> {
        validate_name(filename)?;
        std::fs::create_dir_all(&self.root)
            .map_err(|e| format!("{} : {e}", self.root.display()))?;
        let sound = self.resolve(sound)?;
        variants(&sound, options)
            .iter()
            .map(|v| self.write(&v.sound, &format!("{filename}{}", v.suffix), v.seed))
            .collect()
    }

    /// Comme `export`, puis écrit la recette `filename.json` à côté des WAV.
    #[cfg(feature = "schema")]
    pub fn export_with_recipe(
        &self,
        sound: &Sound,
        filename: &str,
        options: &ExportOptions,
        preset: Option<&str>,
    ) -> Result<Vec<Report>, String> {
        let reports = self.export(sound, filename, options)?;
        let mut recipe = crate::recipe::Recipe::new(sound, preset, options);
        recipe.files = reports.iter().map(|r| r.file_name()).collect();
        let path = self.root.join(format!("{filename}.json"));
        std::fs::write(&path, recipe.to_json()?)
            .map_err(|e| format!("{} : {e}", path.display()))?;
        Ok(reports)
    }

    /// Relit la recette d'un son. Accepte le nom de base ou celui d'une variante (`nom_02`).
    #[cfg(feature = "schema")]
    pub fn read_recipe(&self, filename: &str) -> Result<crate::recipe::Recipe, String> {
        let name = filename
            .strip_suffix(".json")
            .or_else(|| filename.strip_suffix(".wav"))
            .unwrap_or(filename);
        validate_name(name)?;
        let candidates = [Some(name), strip_variant(name)];
        let path = candidates
            .into_iter()
            .flatten()
            .map(|n| self.root.join(format!("{n}.json")))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                format!(
                    "Pas de recette pour « {name} » dans {} (les sons rendus avant la 0.4 n'en ont pas).",
                    self.root.display()
                )
            })?;
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{} : {e}", path.display()))?;
        crate::recipe::Recipe::from_json(&text)
    }

    /// Noms (sans extension) des recettes présentes dans le dossier, triés.
    pub fn list_recipes(&self) -> Vec<String> {
        self.list_with_extension(".json")
    }

    fn write(&self, sound: &Sound, name: &str, seed: u64) -> Result<Report, String> {
        let path = self.root.join(format!("{name}.wav"));
        let samples = render(sound, seed);
        write_wav(&path, &samples, SAMPLE_RATE).map_err(|e| format!("{} : {e}", path.display()))?;
        Ok(Report::from_samples(&path, &samples, SAMPLE_RATE))
    }

    pub fn analyze(&self, filename: &str) -> Result<Report, String> {
        report::analyze_file(&self.existing_wav(filename)?)
    }

    pub fn compare(&self, first: &str, second: &str) -> Result<(Report, Report, Overlap), String> {
        report::compare_files(&self.existing_wav(first)?, &self.existing_wav(second)?)
    }

    pub fn spectrogram_png(&self, filename: &str) -> Result<Vec<u8>, String> {
        report::spectrogram_file(&self.existing_wav(filename)?)
    }

    /// Noms (sans extension) des WAV présents dans le dossier, triés.
    pub fn list_wavs(&self) -> Vec<String> {
        self.list_with_extension(".wav")
    }

    fn list_with_extension(&self, extension: &str) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(extension).map(str::to_owned))
            .collect();
        names.sort();
        names
    }

    /// Un WAV du dossier, ou un enregistrement (« voix/souffle »).
    fn existing_wav(&self, filename: &str) -> Result<PathBuf, String> {
        let name = filename.strip_suffix(".wav").unwrap_or(filename);
        let path = self.recording_path(name)?;
        if !path.is_file() {
            return Err(format!(
                "Fichier introuvable : {name}.wav (dossier {})",
                self.root.display()
            ));
        }
        Ok(path)
    }
}

/// Dossier des morceaux, dans le dossier de travail.
pub const MUSIC_DIR: &str = "musique";

/// La recette d'un morceau : la partition et la graine, de quoi le refaire à l'identique.
#[cfg(feature = "schema")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MusicRecipe {
    pub generator: String,
    pub music: crate::music::Music,
    pub seed: u64,
    pub files: Vec<String>,
}

#[cfg(feature = "schema")]
impl Workspace {
    /// Dossier des instruments : SOUND_INSTRUMENTS_DIR, sinon `instruments/` à côté du dossier de travail.
    pub fn instruments_dir(&self) -> PathBuf {
        std::env::var_os("SOUND_INSTRUMENTS_DIR").map_or_else(
            || self.root.parent().unwrap_or(&self.root).join("instruments"),
            PathBuf::from,
        )
    }

    /// Construit le catalogue des instruments depuis `selection.json` (hauteurs mesurées). Rend un résumé.
    pub fn build_instruments(&self) -> Result<String, String> {
        use crate::instruments::{CATALOGUE_FILE, SELECTION_FILE, Selection, build};
        let dir = self.instruments_dir();
        let text = std::fs::read_to_string(dir.join(SELECTION_FILE))
            .map_err(|e| format!("{} : {e}", dir.join(SELECTION_FILE).display()))?;
        let selection: Selection = serde_json::from_str(&text).map_err(|e| format!("Sélection illisible : {e}"))?;
        let (catalogue, notes) = build(&dir, &selection)?;
        let json = serde_json::to_string_pretty(&catalogue).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(CATALOGUE_FILE), json).map_err(|e| e.to_string())?;
        let mut lines = vec![format!("{} instruments dans {}", catalogue.instruments.len(), dir.join(CATALOGUE_FILE).display())];
        lines.extend(notes);
        Ok(lines.join("\n"))
    }

    pub fn instruments(&self) -> Result<crate::instruments::Catalogue, String> {
        crate::instruments::Catalogue::load(&self.instruments_dir())
    }

    /// Rend un morceau : un WAV stéréo par stem (`musique/<nom>_<stem>.wav`), le mélange (`<nom>_mix.wav`) et
    /// la recette (`<nom>.json`). Rend un rapport par fichier.
    pub fn render_music(&self, music: &crate::music::Music, filename: &str, seed: u64) -> Result<Vec<Report>, String> {
        use crate::music::{mixdown, render_music};
        validate_name(filename)?;
        let dir = self.root.join(MUSIC_DIR);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
        let stems = render_music(music, &self.instruments_dir(), seed)?;
        let mut reports = Vec::new();
        let mix = mixdown(&stems);
        for stem in stems.iter().chain(std::iter::once(&mix)) {
            validate_name(&stem.name)?;
            let path = dir.join(format!("{filename}_{}.wav", stem.name));
            std::fs::write(&path, crate::wav::encode_wav_stereo(&stem.left, &stem.right, SAMPLE_RATE))
                .map_err(|e| format!("{} : {e}", path.display()))?;
            let mono: Vec<f32> = stem.left.iter().zip(&stem.right).map(|(l, r)| (l + r) * 0.5).collect();
            reports.push(Report::from_samples(&path, &mono, SAMPLE_RATE));
        }
        let recipe = MusicRecipe {
            generator: crate::recipe::GENERATOR.to_owned(),
            music: music.clone(),
            seed,
            files: reports.iter().map(|r| format!("{MUSIC_DIR}/{}", r.file_name())).collect(),
        };
        let path = dir.join(format!("{filename}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&recipe).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{} : {e}", path.display()))?;
        Ok(reports)
    }

    pub fn read_music(&self, filename: &str) -> Result<MusicRecipe, String> {
        let name = filename.trim_start_matches("musique/").trim_end_matches(".json");
        validate_name(name)?;
        let path = self.root.join(MUSIC_DIR).join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path).map_err(|_| format!("Pas de morceau « {name} » dans {}.", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("Recette de morceau illisible : {e}"))
    }

    /// Noms des morceaux (recettes) du dossier `musique/`.
    pub fn list_music(&self) -> Vec<String> {
        Workspace::new(self.root.join(MUSIC_DIR)).list_recipes()
    }
}

/// `nom_02` -> `nom` : le suffixe de variante que pose `export`.
#[cfg_attr(not(feature = "schema"), allow(dead_code))]
fn strip_variant(name: &str) -> Option<&str> {
    let (base, index) = name.rsplit_once('_')?;
    (index.len() == 2 && index.bytes().all(|b| b.is_ascii_digit()) && !base.is_empty())
        .then_some(base)
}

/// Nom de fichier sans chemin : lettres, chiffres, `_` et `-` (empêche toute sortie du dossier).
pub fn validate_name(name: &str) -> Result<(), String> {
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if name.is_empty() || name.len() > MAX_NAME_LENGTH || !valid_chars {
        return Err(format!(
            "Nom invalide « {name} » : lettres, chiffres, _ et - uniquement ({MAX_NAME_LENGTH} max)."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    fn temp_workspace(name: &str) -> Workspace {
        let dir = std::env::temp_dir().join(format!("sound_forge_ws_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        Workspace::new(dir)
    }

    #[test]
    fn variants_match_the_exported_files() {
        let workspace = temp_workspace("variants_match");
        let sound = presets::by_name("jump").unwrap();
        let options = ExportOptions {
            variants: 3,
            amount: 0.5,
            seed: 9,
        };
        workspace.export(&sound, "saut", &options).unwrap();
        let set = variants(&sound, &options);
        assert_eq!(set.len(), 3);
        let again = workspace.root().join("rendu_app.wav");
        write_wav(&again, &render_variant(&set[1]), SAMPLE_RATE).unwrap();
        let exported = std::fs::read(workspace.root().join(format!("saut{}.wav", set[1].suffix)));
        assert_eq!(exported.unwrap(), std::fs::read(again).unwrap());
    }

    #[test]
    fn a_recording_is_a_source_like_the_others() {
        let workspace = temp_workspace("recording");
        let voice: Vec<f32> = (0..22_050).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let file = workspace.write_recording("souffle", &voice, 44_100).unwrap();
        assert_eq!(file, "voix/souffle");
        assert_eq!(workspace.list_recordings(), ["voix/souffle"]);
        assert!(workspace.analyze("voix/souffle").is_ok());
        let layer = crate::sound::Layer::new(
            Waveform::Sample(crate::sample::SampleSource::new(&file)),
            crate::envelope::Sweep::new(220.0, 220.0, 0.0),
            crate::envelope::Adsr::new(0.0, 0.0, 1.0, 1.0, 0.01),
        );
        let sound = Sound::new("grave").layer(layer);
        let reports = workspace.export(&sound, "grave", &ExportOptions::default()).unwrap();
        // Une octave plus bas : deux fois plus lent, l'enregistrement d'une demi-seconde dure une seconde.
        assert!((reports[0].stats.duration_s - 1.0).abs() < 0.05, "{}", reports[0].stats.duration_s);
        assert!(reports[0].stats.rms_dbfs > -30.0);
        let missing = Sound::new("x").layer(crate::sound::Layer::new(
            Waveform::Sample(crate::sample::SampleSource::new("voix/absent")),
            crate::envelope::Sweep::constant(440.0),
            crate::envelope::Adsr::percussive(0.0, 0.1),
        ));
        assert!(workspace.export(&missing, "x", &ExportOptions::default()).is_err());
        assert!(workspace.recording_path("../x").is_err() && workspace.recording_path("a/b/c").is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_name("../evil").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("sword_swing-01").is_ok());
    }

    #[test]
    fn export_variants_then_analyze_them() {
        let workspace = temp_workspace("variants");
        let sound = presets::by_name("impact").unwrap();
        let options = ExportOptions {
            variants: 3,
            ..ExportOptions::default()
        };
        let reports = workspace.export(&sound, "hit", &options).unwrap();
        assert_eq!(reports.len(), 3);
        assert_eq!(workspace.list_wavs(), ["hit_01", "hit_02", "hit_03"]);
        let report = workspace.analyze("hit_02.wav").unwrap();
        assert!((report.stats.peak_dbfs - reports[1].stats.peak_dbfs).abs() < 0.1);
    }

    #[test]
    fn compare_and_spectrogram_on_exported_files() {
        let workspace = temp_workspace("compare");
        for name in ["impact", "ui_click"] {
            let sound = presets::by_name(name).unwrap();
            workspace
                .export(&sound, name, &ExportOptions::default())
                .unwrap();
        }
        let (_, _, overlap) = workspace.compare("impact", "ui_click").unwrap();
        assert!(overlap.score < 0.5, "{overlap}");
        assert!(
            workspace
                .spectrogram_png("impact")
                .unwrap()
                .starts_with(b"\x89PNG")
        );
        assert!(workspace.analyze("absent").is_err());
    }
}
