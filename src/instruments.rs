//! Instruments échantillonnés : une bibliothèque d'enregistrements de vrais instruments (VSCO 2 CE, CC0, ou
//! toute autre) décrite par un catalogue JSON, et le choix de l'échantillon à jouer pour une note.
//!
//! Le catalogue se construit une fois (`build`) à partir d'une sélection écrite à la main : pour chaque
//! instrument, un dossier d'enregistrements (la note, la couche de vélocité et la prise alternée se lisent
//! dans le nom des fichiers) ou un kit de percussions (une note par sorte de coup). Les fabricants ne
//! numérotent pas tous les octaves de la même façon : la hauteur réelle de chaque fichier est mesurée, et
//! l'écart d'octave le plus fréquent du dossier corrige les noms.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::wav::read_wav;

/// Ce qu'on écrit à la main : la liste des instruments à tirer de la bibliothèque.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Selection {
    pub instruments: Vec<Wanted>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Wanted {
    pub id: String,
    pub name: String,
    pub family: String,
    /// « tenu » : la note dure ce qu'on lui demande puis s'éteint ; « frappe » : l'échantillon joue en entier.
    pub kind: Kind,
    /// Dossier d'enregistrements nommés par note (relatif au dossier des instruments).
    #[serde(default)]
    pub folder: Option<String>,
    /// Ou un kit : note -> motif de fichiers (« Percussion/BDrumNewhit_* »).
    #[serde(default)]
    pub kit: Vec<(String, String)>,
    /// Extinction après la fin de la note, en secondes (instruments tenus).
    #[serde(default = "default_release")]
    pub release: f32,
    /// Correction de volume pour équilibrer les instruments entre eux.
    #[serde(default)]
    pub gain_db: f32,
    /// Fichiers à écarter (motif), par exemple une couche ratée.
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_release() -> f32 {
    0.3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Tenu,
    Frappe,
}

/// Le catalogue construit : ce que lit le rendu de musique.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Catalogue {
    pub instruments: Vec<Instrument>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Instrument {
    pub id: String,
    pub name: String,
    pub family: String,
    pub kind: Kind,
    pub release: f32,
    pub gain_db: f32,
    /// Calibrage mesuré : ramène la nuance la plus forte de l'instrument au niveau de référence, pour que
    /// les volumes d'une partition veuillent dire la même chose d'un instrument à l'autre.
    #[serde(default)]
    pub level_db: f32,
    /// Notes jouables (MIDI), de la plus grave à la plus aiguë.
    pub range: (u8, u8),
    /// Nombre de couches de vélocité.
    pub layers: u8,
    pub zones: Vec<Zone>,
}

/// Un enregistrement : sa note réelle, sa couche de vélocité (0 = la plus douce), sa prise alternée.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Zone {
    pub file: String,
    pub note: u8,
    pub layer: u8,
    pub rr: u8,
}

pub const CATALOGUE_FILE: &str = "catalogue.json";
pub const SELECTION_FILE: &str = "selection.json";
/// Au-delà, on préfère étirer un autre échantillon : la transposition s'entend.
const MAX_STRETCH: i32 = 7;
/// Niveau efficace (RMS, dBFS) d'une note jouée fort, quel que soit l'instrument ou le son de synthèse.
pub const REFERENCE_DB: f32 = -20.0;

impl Catalogue {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join(CATALOGUE_FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|_| format!("Catalogue d'instruments introuvable : {} (le construire avec build_instruments).", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("Catalogue illisible : {e}"))
    }

    pub fn get(&self, id: &str) -> Option<&Instrument> {
        self.instruments.iter().find(|i| i.id == id)
    }
}

impl Instrument {
    /// L'échantillon pour une note et une vélocité (0..1). `pick` (0..1) choisit parmi les prises alternées.
    /// Rend la zone et l'écart en demi-tons à combler en changeant la vitesse de lecture.
    pub fn zone_for(&self, note: u8, velocity: f32, pick: f32) -> Option<(&Zone, i32)> {
        let nearest = self
            .zones
            .iter()
            .map(|z| (i32::from(note) - i32::from(z.note)).abs())
            .min()?;
        // Parmi les notes enregistrées les plus proches, les couches existantes ; la couche visée par la vélocité.
        let at_note: Vec<&Zone> = self
            .zones
            .iter()
            .filter(|z| (i32::from(note) - i32::from(z.note)).abs() == nearest)
            .collect();
        let root = at_note.iter().map(|z| z.note).min()?;
        let at_root: Vec<&Zone> = at_note.into_iter().filter(|z| z.note == root).collect();
        let mut layers: Vec<u8> = at_root.iter().map(|z| z.layer).collect();
        layers.sort_unstable();
        layers.dedup();
        let wanted = (velocity.clamp(0.0, 0.999) * f32::from(self.layers.max(1))) as u8;
        let layer = *layers.iter().min_by_key(|l| (i32::from(**l) - i32::from(wanted)).abs())?;
        let takes: Vec<&Zone> = at_root.into_iter().filter(|z| z.layer == layer).collect();
        let zone = takes[((pick.clamp(0.0, 0.999)) * takes.len() as f32) as usize];
        Some((zone, i32::from(note) - i32::from(zone.note)))
    }
}

/// Construit le catalogue à partir de la sélection, en mesurant la hauteur réelle des enregistrements.
/// Rend le catalogue et des remarques (octaves corrigés, fichiers non reconnus).
pub fn build(dir: &Path, selection: &Selection) -> Result<(Catalogue, Vec<String>), String> {
    let mut notes = Vec::new();
    let mut instruments = Vec::new();
    for wanted in &selection.instruments {
        let (zones, remark) = if let Some(folder) = &wanted.folder {
            scan_folder(dir, folder, &wanted.exclude)?
        } else {
            (scan_kit(dir, &wanted.kit)?, None)
        };
        if zones.is_empty() {
            notes.push(format!("{} : aucun enregistrement reconnu, écarté.", wanted.id));
            continue;
        }
        if let Some(remark) = remark {
            notes.push(format!("{} : {remark}", wanted.id));
        }
        let low = zones.iter().map(|z| z.note).min().unwrap_or(0);
        let high = zones.iter().map(|z| z.note).max().unwrap_or(127);
        let layers = zones.iter().map(|z| z.layer).max().unwrap_or(0) + 1;
        // Un son frappé s'entend à son attaque ; un son tenu, sur sa durée.
        let window = if wanted.kind == Kind::Frappe { 0.3 } else { 1.0 };
        let level_db = REFERENCE_DB - loudest_level(dir, &zones, layers - 1, window);
        let stretch = if wanted.kind == Kind::Frappe && wanted.folder.is_none() { 0 } else { MAX_STRETCH as u8 };
        instruments.push(Instrument {
            id: wanted.id.clone(),
            name: wanted.name.clone(),
            family: wanted.family.clone(),
            kind: wanted.kind,
            release: wanted.release,
            gain_db: wanted.gain_db,
            level_db,
            range: (low.saturating_sub(stretch), high.saturating_add(stretch).min(127)),
            layers,
            zones,
        });
    }
    Ok((Catalogue { instruments }, notes))
}

fn scan_folder(dir: &Path, folder: &str, exclude: &[String]) -> Result<(Vec<Zone>, Option<String>), String> {
    let path = dir.join(folder);
    let mut files: Vec<String> = std::fs::read_dir(&path)
        .map_err(|e| format!("{} : {e}", path.display()))?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|f| f.to_lowercase().ends_with(".wav") && !exclude.iter().any(|x| matches(x, f)))
        .collect();
    files.sort();
    let parsed: Vec<(String, Parsed)> = files
        .into_iter()
        .filter_map(|f| parse_name(&f).map(|p| (f, p)))
        .collect();
    // Écart d'octave entre le nom et la hauteur réelle : mesuré sur chaque fichier, le plus fréquent l'emporte.
    let mut votes: HashMap<i32, usize> = HashMap::new();
    for (file, p) in &parsed {
        if let Some(measured) = measure_note(&path.join(file)) {
            let octaves = ((measured - f32::from(p.note)) / 12.0).round() as i32;
            // Un fabricant ne décale jamais de plus d'un octave : au-delà, la mesure s'est trompée
            // (harmoniques d'une note grave et brève).
            if octaves.abs() <= 1 {
                *votes.entry(octaves).or_default() += 1;
            }
        }
    }
    let shift = votes.iter().max_by_key(|(octave, count)| (**count, -octave.abs())).map_or(0, |(o, _)| *o);
    let layers = rank(parsed.iter().map(|(_, p)| p.dynamic).collect());
    let zones = parsed
        .iter()
        .filter_map(|(file, p)| {
            let note = i32::from(p.note) + shift * 12;
            u8::try_from(note).ok().filter(|n| *n <= 127).map(|note| Zone {
                file: format!("{folder}/{file}"),
                note,
                layer: layers[&p.dynamic],
                rr: p.rr,
            })
        })
        .collect();
    let remark = (shift != 0).then(|| format!("noms décalés de {shift} octave(s), corrigés d'après la hauteur mesurée"));
    Ok((zones, remark))
}

fn scan_kit(dir: &Path, kit: &[(String, String)]) -> Result<Vec<Zone>, String> {
    let mut zones = Vec::new();
    for (note_name, pattern) in kit {
        let note = parse_note(note_name).ok_or_else(|| format!("Note de kit illisible : {note_name}"))?;
        let (folder, file_pattern) = pattern.rsplit_once('/').unwrap_or(("", pattern));
        let path = dir.join(folder);
        let mut files: Vec<String> = std::fs::read_dir(&path)
            .map_err(|e| format!("{} : {e}", path.display()))?
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|f| matches(file_pattern, f))
            .collect();
        files.sort();
        let parsed: Vec<Parsed> = files.iter().map(|f| Parsed { note, dynamic: dynamic_of(f), rr: rr_of(f) }).collect();
        let layers = rank(parsed.iter().map(|p| p.dynamic).collect());
        for (file, p) in files.iter().zip(&parsed) {
            zones.push(Zone {
                file: if folder.is_empty() { file.clone() } else { format!("{folder}/{file}") },
                note,
                layer: layers[&p.dynamic],
                rr: p.rr,
            });
        }
    }
    Ok(zones)
}

/// Niveau efficace médian (dBFS) des enregistrements de la nuance la plus forte, sur leurs `window` premières
/// secondes.
fn loudest_level(dir: &Path, zones: &[Zone], top: u8, window: f32) -> f32 {
    let mut levels: Vec<f32> = zones
        .iter()
        .filter(|z| z.layer == top)
        .filter_map(|z| {
            let (samples, rate) = read_wav(&dir.join(&z.file)).ok()?;
            let body = &samples[..samples.len().min((window * rate as f32) as usize)];
            let rms = (body.iter().map(|s| s * s).sum::<f32>() / body.len().max(1) as f32).sqrt();
            (rms > 1e-6).then(|| 20.0 * rms.log10())
        })
        .collect();
    levels.sort_by(f32::total_cmp);
    levels.get(levels.len() / 2).copied().unwrap_or(REFERENCE_DB)
}

/// Rang de chaque nuance trouvée (0 = la plus douce).
fn rank(mut values: Vec<u16>) -> BTreeMap<u16, u8> {
    values.sort_unstable();
    values.dedup();
    values.into_iter().enumerate().map(|(i, v)| (v, i as u8)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Parsed {
    note: u8,
    /// Nuance comparable : `v3` -> 3, `pp` -> 2… (seul l'ordre compte).
    dynamic: u16,
    rr: u8,
}

/// Lit note, nuance et prise dans un nom comme `MOHorn_sus_A#1_v2_1.wav` ou `UR1_C4_mf_RR2.wav`.
fn parse_name(file: &str) -> Option<Parsed> {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let tokens: Vec<&str> = stem.split(['_', '-', ' ']).collect();
    let note = tokens.iter().find_map(|t| parse_note(t))?;
    Some(Parsed { note, dynamic: dynamic_of(file), rr: rr_of(file) })
}

const DYNAMICS: [&str; 8] = ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"];

fn dynamic_of(file: &str) -> u16 {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    for token in stem.split(['_', '-', ' ']) {
        let lower = token.to_lowercase();
        if let Some(n) = lower.strip_prefix('v').and_then(|n| n.parse::<u16>().ok()) {
            return 100 + n;
        }
        if let Some(n) = lower.strip_prefix("dyn").and_then(|n| n.parse::<u16>().ok()) {
            return 100 + n;
        }
        if let Some(i) = DYNAMICS.iter().position(|d| *d == lower) {
            return 10 + i as u16;
        }
        if lower == "loud" {
            return 16;
        }
        if lower == "soft" || lower == "quiet" {
            return 12;
        }
    }
    0
}

fn rr_of(file: &str) -> u8 {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let tokens: Vec<&str> = stem.split(['_', '-', ' ']).collect();
    for token in &tokens {
        let lower = token.to_lowercase();
        if let Some(n) = lower.strip_prefix("rr").and_then(|n| n.parse::<u8>().ok()) {
            return n;
        }
    }
    // Sinon le dernier nombre seul (« _1 », « _01 »), s'il suit la nuance.
    tokens.iter().rev().find_map(|t| t.parse::<u8>().ok()).unwrap_or(1)
}

/// « A#1 », « Bb3 », « C-1 » -> numéro MIDI (A4 = 69).
pub fn parse_note(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    let base = match bytes.first()?.to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let (accidental, rest) = match bytes.get(1) {
        Some(b'#') => (1, &text[2..]),
        Some(b'b') if text.len() > 2 && (text[2..].parse::<i32>().is_ok()) => (-1, &text[2..]),
        _ => (0, &text[1..]),
    };
    let octave: i32 = rest.parse().ok()?;
    if !(-1..=9).contains(&octave) {
        return None;
    }
    u8::try_from((octave + 1) * 12 + base + accidental).ok().filter(|n| *n <= 127)
}

/// Motif simple : `*` remplace n'importe quelle suite de caractères.
fn matches(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let mut rest = name;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            let Some(r) = rest.strip_prefix(part) else { return false };
            rest = r;
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            let Some(at) = rest.find(part) else { return false };
            rest = &rest[at + part.len()..];
        }
    }
    true
}

/// Note MIDI mesurée (fractionnaire) d'un enregistrement : autocorrélation sur 0,4 s après l'attaque.
fn measure_note(path: &PathBuf) -> Option<f32> {
    let (samples, rate) = read_wav(path).ok()?;
    let rate_f = rate as f32;
    let start = ((0.15 * rate_f) as usize).min(samples.len());
    let end = ((0.55 * rate_f) as usize).min(samples.len());
    let segment = &samples[start..end];
    if segment.len() < 2048 {
        return None;
    }
    // Sous-échantillonné par 2 : assez précis pour trancher un octave, deux fois plus rapide.
    let x: Vec<f32> = segment.iter().step_by(2).copied().collect();
    let rate_d = rate_f / 2.0;
    let mean = x.iter().sum::<f32>() / x.len() as f32;
    let x: Vec<f32> = x.iter().map(|v| v - mean).collect();
    let energy: f32 = x.iter().map(|v| v * v).sum();
    if energy < 1e-6 {
        return None;
    }
    let (low, high) = ((rate_d / 2000.0) as usize, ((rate_d / 25.0) as usize).min(x.len() / 2));
    let corr = |lag: usize| x[..x.len() - lag].iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f32>();
    let values: Vec<f32> = (low..high).map(corr).collect();
    let best = values.iter().copied().fold(f32::MIN, f32::max);
    // Le premier pic proche du maximum : évite de prendre l'octave inférieur (période double).
    let lag = values.iter().position(|v| *v >= best * 0.9)? + low;
    let freq = rate_d / lag as f32;
    Some(69.0 + 12.0 * (freq / 440.0).log2())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_notes_dynamics_and_takes_from_names() {
        assert_eq!(parse_note("A4"), Some(69));
        assert_eq!(parse_note("A#1"), Some(34));
        assert_eq!(parse_note("Bb3"), Some(58));
        assert_eq!(parse_note("C-1"), Some(0));
        assert_eq!(parse_note("Sum"), None);
        assert_eq!(parse_note("Bassoon"), None);
        let horn = parse_name("MOHorn_sus_A#1_v2_1.wav").unwrap();
        assert_eq!((horn.note, horn.dynamic, horn.rr), (34, 102, 1));
        let piano = parse_name("UR1_C4_mf_RR2.wav").unwrap();
        assert_eq!((piano.note, piano.dynamic, piano.rr), (60, 14, 2));
        let marimba = parse_name("Marimba_hit_Outrigger_C2_loud_01.wav").unwrap();
        assert_eq!((marimba.note, marimba.rr), (36, 1));
        assert!(parse_name("NT5_Man3Quiet_122_rr1.wav").is_none());
    }

    #[test]
    fn picks_the_nearest_recorded_note_then_the_layer_then_a_take() {
        let zones = vec![
            Zone { file: "a".into(), note: 60, layer: 0, rr: 1 },
            Zone { file: "b".into(), note: 60, layer: 1, rr: 1 },
            Zone { file: "c".into(), note: 60, layer: 1, rr: 2 },
            Zone { file: "d".into(), note: 64, layer: 0, rr: 1 },
        ];
        let inst = Instrument { id: "x".into(), name: "x".into(), family: "x".into(), kind: Kind::Tenu, release: 0.3, gain_db: 0.0, level_db: 0.0, range: (53, 71), layers: 2, zones };
        let (z, shift) = inst.zone_for(61, 0.9, 0.0).unwrap();
        assert_eq!((z.file.as_str(), shift), ("b", 1));
        assert_eq!(inst.zone_for(61, 0.9, 0.99).unwrap().0.file, "c");
        assert_eq!(inst.zone_for(61, 0.1, 0.5).unwrap().0.file, "a");
        // Note manquante à cette couche : la couche la plus proche disponible.
        let (z, shift) = inst.zone_for(65, 0.9, 0.0).unwrap();
        assert_eq!((z.file.as_str(), shift), ("d", 1));
    }

    #[test]
    fn simple_patterns() {
        assert!(matches("BDrumNewhit_*", "BDrumNewhit_v1_rr1_Sum.wav"));
        assert!(matches("*_Sum.wav", "a_Sum.wav"));
        assert!(matches("Tri*Hit_*", "Triangle3-Hit_v1_rr1_Sum.wav"));
        assert!(!matches("gongHit*", "gongscrape_mf.wav"));
    }
}
