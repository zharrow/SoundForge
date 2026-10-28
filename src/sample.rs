//! Enregistrements utilisés comme source d'une couche : une voix, un bruit de bouche, un objet frappé.
//!
//! La recette ne contient que le nom du fichier (`voix/souffle` pour `voix/souffle.wav` dans le dossier
//! de travail) ; les échantillons sont chargés au moment du rendu par [`crate::Workspace`], gardés en
//! mémoire tant que le fichier ne change pas, et rééchantillonnés à 44,1 kHz.
//!
//! La hauteur de la couche règle la vitesse de lecture : 440 Hz = vitesse d'origine, 880 Hz = deux fois
//! plus vite (une octave plus haut), 220 Hz = deux fois plus lent. Balayages, vibrato, unisson, filtre,
//! enveloppe et résonateur s'appliquent comme aux autres sources.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use crate::render::SAMPLE_RATE;
use crate::wav::read_wav;

/// Hauteur à laquelle un enregistrement est lu à sa vitesse d'origine.
pub const ROOT_HZ: f32 = 440.0;
/// Dossier des enregistrements, dans le dossier de travail.
pub const RECORDINGS_DIR: &str = "voix";

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct SampleSource {
    /// Enregistrement du dossier de travail, sans extension : « voix/souffle » pour voix/souffle.wav.
    pub file: String,
    /// Lu à l'envers (la queue d'abord) : aspirations, montées étranges.
    #[cfg_attr(feature = "schema", serde(default))]
    pub reverse: bool,
    /// Échantillons chargés au rendu ; jamais écrits dans la recette.
    #[cfg_attr(feature = "schema", serde(skip))]
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub data: SampleData,
}

impl SampleSource {
    pub fn new(file: &str) -> Self {
        Self {
            file: file.to_owned(),
            reverse: false,
            data: SampleData::default(),
        }
    }

    /// Durée de l'enregistrement chargé à vitesse d'origine (0 s s'il n'est pas chargé).
    pub fn duration(&self) -> f32 {
        self.data.0.as_ref().map_or(0.0, |d| d.len() as f32 / SAMPLE_RATE as f32)
    }
}

/// Les échantillons d'un enregistrement, partagés entre variantes. Deux sons qui ne diffèrent que par
/// ce cache sont égaux : il se déduit du fichier.
#[derive(Clone, Default)]
pub struct SampleData(pub Option<Arc<Vec<f32>>>);

impl PartialEq for SampleData {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl fmt::Debug for SampleData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(data) => write!(f, "{} échantillons", data.len()),
            None => f.write_str("non chargé"),
        }
    }
}

/// Lecteur d'un enregistrement : avance à la vitesse demandée, interpolation linéaire, silence après la fin.
#[derive(Debug, Clone)]
pub struct SamplePlayer {
    data: Option<Arc<Vec<f32>>>,
    reverse: bool,
    position: f64,
}

impl SamplePlayer {
    pub fn new(source: &SampleSource) -> Self {
        Self {
            data: source.data.0.clone(),
            reverse: source.reverse,
            position: 0.0,
        }
    }

    pub fn next(&mut self, freq: f32, sample_rate: f32) -> f32 {
        let Some(data) = &self.data else { return 0.0 };
        let len = data.len();
        let index = self.position.floor() as usize;
        let value = if index + 1 < len {
            let fraction = (self.position - index as f64) as f32;
            let (a, b) = if self.reverse {
                (data[len - 1 - index], data[len - 2 - index])
            } else {
                (data[index], data[index + 1])
            };
            a + (b - a) * fraction
        } else {
            0.0
        };
        self.position += f64::from((freq / ROOT_HZ) * (SAMPLE_RATE as f32 / sample_rate)).max(0.0);
        value
    }
}

type Cached = (SystemTime, u64, Arc<Vec<f32>>);
static CACHE: LazyLock<Mutex<HashMap<PathBuf, Cached>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Charge un WAV en mono 44,1 kHz, depuis le cache si le fichier n'a pas changé.
pub fn load(path: &Path) -> Result<Arc<Vec<f32>>, String> {
    let meta = std::fs::metadata(path).map_err(|_| format!("Enregistrement introuvable : {}", path.display()))?;
    let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
    if let Some((time, len, data)) = CACHE.lock().unwrap().get(path) {
        if (*time, *len) == stamp {
            return Ok(data.clone());
        }
    }
    let (samples, rate) = read_wav(path)?;
    let data = Arc::new(resample(&samples, rate, SAMPLE_RATE));
    CACHE
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), (stamp.0, stamp.1, data.clone()));
    Ok(data)
}

/// Rééchantillonnage linéaire : suffisant pour une voix enregistrée à 44,1 ou 48 kHz.
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let count = (samples.len() as u64 * u64::from(to) / u64::from(from)) as usize;
    (0..count)
        .map(|i| {
            let position = i as f64 * ratio;
            let index = (position.floor() as usize).min(samples.len() - 1);
            let fraction = (position - index as f64) as f32;
            let a = samples[index];
            let b = *samples.get(index + 1).unwrap_or(&a);
            a + (b - a) * fraction
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(data: Vec<f32>, reverse: bool) -> SampleSource {
        SampleSource {
            file: "voix/test".into(),
            reverse,
            data: SampleData(Some(Arc::new(data))),
        }
    }

    #[test]
    fn plays_at_original_speed_then_falls_silent() {
        let mut player = SamplePlayer::new(&source(vec![0.0, 0.5, 1.0, 0.5], false));
        let out: Vec<f32> = (0..6).map(|_| player.next(ROOT_HZ, SAMPLE_RATE as f32)).collect();
        assert_eq!(out, [0.0, 0.5, 1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn an_octave_up_reads_twice_as_fast_and_reverse_starts_from_the_end() {
        let mut fast = SamplePlayer::new(&source(vec![0.0, 0.1, 0.2, 0.3, 0.4], false));
        let out: Vec<f32> = (0..3).map(|_| fast.next(2.0 * ROOT_HZ, SAMPLE_RATE as f32)).collect();
        assert_eq!(out, [0.0, 0.2, 0.0]);
        let mut back = SamplePlayer::new(&source(vec![0.0, 0.1, 0.2], true));
        assert!((back.next(ROOT_HZ, SAMPLE_RATE as f32) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn resampling_keeps_the_duration() {
        let one_second: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin()).collect();
        assert_eq!(resample(&one_second, 48_000, 44_100).len(), 44_100);
    }
}
