//! Dossier de travail partagé par la CLI et le serveur MCP.
//! Tous les fichiers lus ou écrits restent confinés dans ce dossier.

use std::path::{Path, PathBuf};

use crate::bands::Overlap;
use crate::mutate::vary;
use crate::render::{SAMPLE_RATE, render};
use crate::report::{self, Report};
use crate::rng::Rng;
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
        if options.variants <= 1 {
            return Ok(vec![self.write(sound, filename, options.seed)?]);
        }
        let mut rng = Rng::new(options.seed);
        (1..=options.variants.min(MAX_VARIANTS))
            .map(|index| {
                let variant = vary(sound, options.amount, &mut rng);
                let seed = options.seed + u64::from(index);
                self.write(&variant, &format!("{filename}_{index:02}"), seed)
            })
            .collect()
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
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|name| name.strip_suffix(".wav").map(str::to_owned))
            .collect();
        names.sort();
        names
    }

    fn existing_wav(&self, filename: &str) -> Result<PathBuf, String> {
        let name = filename.strip_suffix(".wav").unwrap_or(filename);
        validate_name(name)?;
        let path = self.root.join(format!("{name}.wav"));
        if !path.is_file() {
            return Err(format!(
                "Fichier introuvable : {name}.wav (dossier {})",
                self.root.display()
            ));
        }
        Ok(path)
    }
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
