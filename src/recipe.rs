//! La recette d'un son : de quoi le refaire à l'identique.
//!
//! Un WAV seul ne se retouche pas : pour changer la brillance d'une cloche faite il y a un
//! mois, il faudrait réécrire son JSON de mémoire. La recette est donc écrite à côté des WAV,
//! `nom.json` pour `nom.wav` ou `nom_01.wav`, `nom_02.wav`… avec la graine et les réglages de
//! variantes. Même binaire + même recette = mêmes fichiers, octet pour octet (une compilation
//! de débogage peut arrondir autrement quelques échantillons, d'un bit de poids faible).

use crate::sound::Sound;
use crate::workspace::ExportOptions;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Recipe {
    /// Moteur qui a rendu les fichiers. Une autre version peut rendre un son légèrement différent.
    pub generator: String,
    /// Preset d'origine, si le son en vient tel quel.
    #[serde(default)]
    pub preset: Option<String>,
    pub sound: Sound,
    pub variants: u32,
    pub amount: f32,
    pub seed: u64,
    /// WAV produits par cette recette, sans chemin.
    #[serde(default)]
    pub files: Vec<String>,
}

pub const GENERATOR: &str = concat!("sound-forge ", env!("CARGO_PKG_VERSION"));

impl Recipe {
    pub fn new(sound: &Sound, preset: Option<&str>, options: &ExportOptions) -> Self {
        Self {
            generator: GENERATOR.to_owned(),
            preset: preset.map(str::to_owned),
            sound: sound.clone(),
            variants: options.variants,
            amount: options.amount,
            seed: options.seed,
            files: Vec::new(),
        }
    }

    pub fn options(&self) -> ExportOptions {
        ExportOptions {
            variants: self.variants,
            amount: self.amount,
            seed: self.seed,
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("Recette illisible : {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;
    use crate::workspace::Workspace;

    #[test]
    fn a_recipe_renders_the_same_files_again() {
        let dir = std::env::temp_dir().join("sound_forge_recipe_roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::new(&dir);
        let options = ExportOptions {
            variants: 2,
            amount: 0.4,
            seed: 5,
        };
        let sound = presets::by_name("wood_knock").unwrap();
        ws.export_with_recipe(&sound, "toc", &options, Some("wood_knock"))
            .unwrap();
        let first = std::fs::read(dir.join("toc_02.wav")).unwrap();

        let recipe = ws.read_recipe("toc_02").unwrap();
        assert_eq!(recipe.files, ["toc_01.wav", "toc_02.wav"]);
        assert_eq!(recipe.preset.as_deref(), Some("wood_knock"));
        std::fs::remove_file(dir.join("toc_02.wav")).unwrap();
        ws.export_with_recipe(&recipe.sound, "toc", &recipe.options(), None)
            .unwrap();
        assert_eq!(std::fs::read(dir.join("toc_02.wav")).unwrap(), first);
    }

    #[test]
    fn a_missing_or_broken_recipe_is_reported() {
        let dir = std::env::temp_dir().join("sound_forge_recipe_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ws = Workspace::new(&dir);
        assert!(ws.read_recipe("absent").is_err());
        std::fs::write(dir.join("casse.json"), "{pas du json").unwrap();
        assert!(ws.read_recipe("casse").is_err());
        assert!(ws.read_recipe("../ailleurs").is_err());
    }
}
