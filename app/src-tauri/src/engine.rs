//! Commandes du moteur pour la page : rendre une prise, lire et écrire le dossier de travail.
//!
//! La page tient le son en cours d'édition ; elle envoie ici une « prise » (le son et ses réglages de
//! variantes) et reçoit le WAV à écouter, le spectrogramme et les mesures. Le rendu passe par les
//! mêmes fonctions que l'export : ce qu'on entend est ce qui sera écrit.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sound_forge::bands::{BANDS, band_profile};
use sound_forge::recipe::Recipe;
use sound_forge::{
    ExportOptions, SAMPLE_RATE, Sound, Workspace, analyze, base64, encode_wav, presets,
    render_variant, spectrogram, variants, wav::parse_wav, workspace::validate_name,
};

/// Le son et ses réglages de variantes : ce qu'une recette enregistre.
#[derive(Debug, Clone, Deserialize)]
pub struct Take {
    pub sound: Sound,
    pub variants: u32,
    pub amount: f32,
    pub seed: u64,
}

impl Take {
    fn options(&self) -> ExportOptions {
        ExportOptions {
            variants: self.variants.max(1),
            amount: self.amount,
            seed: self.seed,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Rendered {
    /// WAV 16 bits mono, en base64.
    wav: String,
    /// Spectrogramme seul (480 × 240), PNG en base64.
    spectrum: String,
    /// Image de contrôle complète (forme d'onde + spectrogramme), celle que voit Claude ; sur demande.
    overview: Option<String>,
    duration: f32,
    peak_db: f32,
    rms_db: f32,
    /// Part de l'énergie par bande, avec le nom des bandes.
    bands: Vec<(String, f32)>,
    /// Début et fin (queue du résonateur comprise) de chaque couche de la variante rendue, en secondes.
    layers: Vec<(f32, f32)>,
    /// Nombre de versions de la prise (1, ou le nombre de variantes).
    count: usize,
    /// Suffixe de fichier de la version rendue : "", "_01", "_02"…
    suffix: String,
}

pub fn workspace() -> Workspace {
    Workspace::new(workspace_root())
}

/// SOUND_OUTPUT_DIR, sinon ~/GameAssets/sfx : le dossier du serveur MCP.
pub fn workspace_root() -> PathBuf {
    std::env::var_os("SOUND_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::home_dir().join("GameAssets").join("sfx"))
}

/// Rend la version `variant` (0 = la première) d'une prise.
pub fn render_take(take: &Take, variant: usize, overview: bool) -> Result<Rendered, String> {
    let sound = workspace().resolve(&take.sound)?;
    sound.check()?;
    let set = variants(&sound, &take.options());
    let chosen = set
        .get(variant)
        .ok_or_else(|| format!("Variante {} inexistante ({} au total).", variant + 1, set.len()))?;
    let samples = render_variant(chosen);
    let stats = analyze(&samples, SAMPLE_RATE);
    let profile = band_profile(&samples, SAMPLE_RATE);
    Ok(Rendered {
        wav: base64::encode(&encode_wav(&samples, SAMPLE_RATE)),
        spectrum: base64::encode(&spectrogram::render_spectrum_png(&samples, SAMPLE_RATE)),
        overview: overview.then(|| base64::encode(&spectrogram::render_png(&samples, SAMPLE_RATE))),
        duration: stats.duration_s,
        peak_db: stats.peak_dbfs,
        rms_db: stats.rms_dbfs,
        bands: BANDS
            .iter()
            .zip(profile.0)
            .map(|(band, share)| (band.name.to_owned(), share))
            .collect(),
        layers: chosen
            .sound
            .layers
            .iter()
            .map(|layer| (layer.start, layer.end_time()))
            .collect(),
        count: set.len(),
        suffix: chosen.suffix.clone(),
    })
}

#[tauri::command]
pub async fn render(take: Take, variant: usize, overview: Option<bool>) -> Result<Rendered, String> {
    tauri::async_runtime::spawn_blocking(move || render_take(&take, variant, overview.unwrap_or(false)))
        .await
        .map_err(|error| error.to_string())?
}

#[derive(Debug, Serialize)]
pub struct Entry {
    name: String,
    /// Recette présente : le son est retouchable.
    editable: bool,
    variants: u32,
    files: Vec<String>,
    preset: Option<String>,
    generator: Option<String>,
    /// Dernière modification (ms depuis 1970), recette ou WAV le plus récent.
    modified: u64,
}

#[derive(Debug, Serialize)]
pub struct Library {
    folder: String,
    sounds: Vec<Entry>,
    presets: Vec<(String, String)>,
    /// Enregistrements (« voix/souffle ») et leur durée en secondes.
    recordings: Vec<(String, f32)>,
}

#[tauri::command]
pub fn library() -> Library {
    let ws = workspace();
    let wavs = ws.list_wavs();
    let mut covered = std::collections::HashSet::new();
    let mut sounds: Vec<Entry> = ws
        .list_recipes()
        .into_iter()
        .filter_map(|name| {
            let recipe = ws.read_recipe(&name).ok()?;
            let files: Vec<String> = if recipe.files.is_empty() {
                wavs.iter().filter(|w| is_variant_of(w, &name)).map(|w| format!("{w}.wav")).collect()
            } else {
                recipe.files.clone()
            };
            covered.extend(files.iter().map(|f| f.trim_end_matches(".wav").to_owned()));
            Some(Entry {
                modified: newest(&ws, &name, &files),
                name,
                editable: true,
                variants: recipe.variants,
                files,
                preset: recipe.preset,
                generator: Some(recipe.generator),
            })
        })
        .collect();
    // Des WAV sans recette (rendus avant la 0.4, ou venus d'ailleurs) : listés, mais pas retouchables.
    let mut loose: Vec<String> = wavs.into_iter().filter(|w| !covered.contains(w)).collect();
    loose.sort();
    for wav in loose {
        let base = strip_variant(&wav).to_owned();
        if let Some(entry) = sounds.iter_mut().find(|e| !e.editable && e.name == base) {
            entry.files.push(format!("{wav}.wav"));
            entry.variants += 1;
            continue;
        }
        let files = vec![format!("{wav}.wav")];
        sounds.push(Entry {
            modified: newest(&ws, &base, &files),
            name: base,
            editable: false,
            variants: 1,
            files,
            preset: None,
            generator: None,
        });
    }
    sounds.sort_by(|a, b| a.name.cmp(&b.name));
    Library {
        folder: ws.root().to_string_lossy().into_owned(),
        sounds,
        presets: presets::CATALOG
            .iter()
            .map(|p| (p.name.to_owned(), p.description.to_owned()))
            .collect(),
        recordings: ws
            .list_recordings()
            .into_iter()
            .map(|name| {
                let seconds = ws.analyze(&name).map_or(0.0, |r| r.stats.duration_s);
                (name, seconds)
            })
            .collect(),
    }
}

/// Enregistre une prise de micro (WAV en base64, déjà rognée par la page) dans `voix/`.
#[tauri::command]
pub async fn save_recording(name: String, wav: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (samples, rate) = parse_wav(&base64::decode(&wav)?)?;
        if samples.is_empty() {
            return Err("Enregistrement vide.".into());
        }
        workspace().write_recording(&name, &samples, rate)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Un enregistrement en WAV (base64), pour l'écouter ou le dessiner dans la page.
#[tauri::command]
pub fn recording_wav(name: String) -> Result<String, String> {
    let path = workspace().recording_path(&name)?;
    std::fs::read(&path)
        .map(|bytes| base64::encode(&bytes))
        .map_err(|_| format!("Enregistrement introuvable : {name}"))
}

fn is_variant_of(wav: &str, base: &str) -> bool {
    wav == base || strip_variant(wav) == base && wav != strip_variant(wav)
}

/// `nom_02` -> `nom`.
fn strip_variant(name: &str) -> &str {
    match name.rsplit_once('_') {
        Some((base, index)) if index.len() == 2 && index.bytes().all(|b| b.is_ascii_digit()) && !base.is_empty() => base,
        _ => name,
    }
}

fn newest(ws: &Workspace, name: &str, files: &[String]) -> u64 {
    std::iter::once(format!("{name}.json"))
        .chain(files.iter().cloned())
        .filter_map(|f| std::fs::metadata(ws.root().join(f)).ok()?.modified().ok())
        .filter_map(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .max()
        .unwrap_or(0)
}

#[derive(Debug, Serialize)]
pub struct Opened {
    name: String,
    sound: Sound,
    variants: u32,
    amount: f32,
    seed: u64,
    preset: Option<String>,
    generator: String,
}

#[tauri::command]
pub fn load_recipe(name: String) -> Result<Opened, String> {
    let recipe: Recipe = workspace().read_recipe(&name)?;
    Ok(Opened {
        name: strip_variant(name.trim_end_matches(".json").trim_end_matches(".wav")).to_owned(),
        sound: recipe.sound,
        variants: recipe.variants,
        amount: recipe.amount,
        seed: recipe.seed,
        preset: recipe.preset,
        generator: recipe.generator,
    })
}

#[tauri::command]
pub fn preset(name: String) -> Result<Sound, String> {
    presets::by_name(&name).ok_or_else(|| format!("Preset inconnu : {name}"))
}

#[derive(Debug, Serialize)]
pub struct Saved {
    files: Vec<String>,
}

/// Écrit les WAV et la recette, exactement comme le serveur MCP.
#[tauri::command]
pub async fn save(name: String, take: Take, preset: Option<String>) -> Result<Saved, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_name(&name)?;
        take.sound.check()?;
        let ws = workspace();
        // Changer le nombre de variantes laisserait des fichiers d'une autre prise : ils sont retirés.
        let before: Vec<String> = ws.read_recipe(&name).map(|r| r.files).unwrap_or_default();
        let reports = ws.export_with_recipe(&take.sound, &name, &take.options(), preset.as_deref())?;
        let files: Vec<String> = reports.iter().map(|r| r.file_name()).collect();
        for stale in before.iter().filter(|f| !files.contains(f)) {
            if validate_name(stale.trim_end_matches(".wav")).is_ok() {
                let _ = std::fs::remove_file(ws.root().join(stale));
            }
        }
        Ok(Saved { files })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn exists(name: String) -> bool {
    validate_name(&name).is_ok() && workspace().root().join(format!("{name}.json")).is_file()
}

/// Montre le son dans le Finder.
#[tauri::command]
pub fn reveal(name: Option<String>) -> Result<(), String> {
    let root = workspace_root();
    let target = match name {
        Some(name) => {
            validate_name(&name)?;
            let recipe = root.join(format!("{name}.json"));
            if recipe.is_file() { recipe } else { root }
        }
        None => root,
    };
    std::process::Command::new("open")
        .arg("-R")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(variants: u32) -> Take {
        Take {
            sound: presets::by_name("impact").unwrap(),
            variants,
            amount: 0.4,
            seed: 3,
        }
    }

    #[test]
    fn renders_each_variant_with_its_suffix() {
        let first = render_take(&take(3), 0, false).unwrap();
        let third = render_take(&take(3), 2, true).unwrap();
        assert_eq!((first.count, first.suffix.as_str()), (3, "_01"));
        assert_eq!(third.suffix, "_03");
        assert!(third.overview.is_some() && first.overview.is_none());
        assert_ne!(first.wav, third.wav);
        assert!(render_take(&take(3), 3, false).is_err());
        let single = render_take(&take(1), 0, false).unwrap();
        assert_eq!((single.count, single.suffix.as_str()), (1, ""));
        assert_eq!(single.bands.len(), 7);
        assert!((single.peak_db + 1.0).abs() < 0.1, "crête {}", single.peak_db);
    }

    #[test]
    fn variant_names_strip_only_two_digits() {
        assert_eq!(strip_variant("pas_03"), "pas");
        assert_eq!(strip_variant("coup_final"), "coup_final");
        assert_eq!(strip_variant("x_123"), "x_123");
        assert!(is_variant_of("pas_03", "pas") && is_variant_of("pas", "pas"));
        assert!(!is_variant_of("pas_lourd", "pas"));
    }
}
