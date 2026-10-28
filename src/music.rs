//! Musique : une partition (tempo, mesures, pistes de notes) jouée par des instruments échantillonnés ou
//! par des sons de synthèse, rendue en stems stéréo synchronisés.
//!
//! Règles de la direction audio d'Eku (GDD, skill audio) :
//! - tous les stems d'un morceau ont exactement la même longueur en échantillons et partent ensemble ;
//! - une boucle se raccorde : ce qui résonne après la dernière mesure revient sur le début (queue cyclique) ;
//! - les stems ne sont jamais normalisés chacun de leur côté : un seul gain, calculé sur leur somme.
//! Même partition + même graine + mêmes enregistrements = mêmes fichiers.

use std::collections::{BTreeMap, HashMap};
use std::f32::consts::FRAC_PI_4;
use std::path::Path;

use crate::effects::{Reverb, reverb_stereo};
use crate::instruments::{Catalogue, Kind, REFERENCE_DB, parse_note};
use crate::render::{SAMPLE_RATE, render};
use crate::rng::Rng;
use crate::sample;
use crate::sound::Sound;

const MAX_SECONDS: f32 = 300.0;
const MAX_NOTES: usize = 20_000;
const MAX_TAIL: f32 = 6.0;
/// Une note frappée ne joue pas plus longtemps que ça (un gong, une cymbale).
const MAX_ONE_SHOT: f32 = 10.0;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Music {
    pub name: String,
    /// Battements par minute.
    pub bpm: f32,
    /// Temps par mesure (4 = 4/4, 3 = 3/4).
    #[serde(default = "four")]
    pub beats_per_bar: u32,
    /// Longueur du morceau, en mesures : c'est la boucle.
    pub bars: u32,
    /// Morceau en boucle : la queue revient sur le début. Faux pour un jingle (stinger).
    #[serde(default = "yes")]
    pub looping: bool,
    /// Motifs réutilisables (leitmotivs, ostinatos) : nom -> notes, placées à partir du temps 0.
    #[serde(default)]
    pub patterns: BTreeMap<String, Vec<Note>>,
    pub tracks: Vec<Track>,
    /// L'espace commun (réverbération partagée par toutes les pistes, dosée par piste).
    #[serde(default)]
    pub reverb: Option<Reverb>,
    /// Crête du mélange de tous les stems, en dBFS. Les stems gardent leur équilibre.
    #[serde(default = "default_peak")]
    pub peak_db: f32,
}

fn four() -> u32 {
    4
}
fn yes() -> bool {
    true
}
fn default_peak() -> f32 {
    -1.0
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Track {
    pub name: String,
    /// Stem auquel la piste appartient (« ambience », « tension », « combat », « melody »…). Défaut : son nom.
    #[serde(default)]
    pub stem: Option<String>,
    /// Instrument du catalogue (list_instruments), ou un son de synthèse joué à la hauteur de chaque note.
    pub instrument: Voice,
    #[serde(default)]
    pub volume_db: f32,
    /// Placement gauche (-1) / droite (1).
    #[serde(default)]
    pub pan: f32,
    /// Part envoyée dans l'espace commun, de 0 (sec) à 1.
    #[serde(default = "default_send")]
    pub reverb: f32,
    /// Transposition de toute la piste, en demi-tons.
    #[serde(default)]
    pub transpose: i32,
    /// Jeu humain : petites variations de moment et de force (0 = mécanique, 1 = lâche).
    #[serde(default = "default_humanize")]
    pub humanize: f32,
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Motifs joués par la piste.
    #[serde(default)]
    pub play: Vec<PatternUse>,
}

fn default_send() -> f32 {
    0.25
}
fn default_humanize() -> f32 {
    0.3
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Voice {
    /// Identifiant du catalogue : « violons », « harpe »…
    Sampled(String),
    /// Un son (même format que render_sound) : sa première couche donne la hauteur de référence.
    Synth { synth: Sound },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Note {
    /// Début, en temps depuis le début du morceau (ou du motif).
    pub at: f32,
    /// Durée, en temps.
    pub len: f32,
    /// Hauteur : « D4 », « F#3 », « Bb2 » (la3 = A4 = 440 Hz), ou un numéro MIDI.
    pub note: Pitch,
    /// Force, de 0 à 1.
    #[serde(default = "default_vel")]
    pub vel: f32,
}

fn default_vel() -> f32 {
    0.7
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Pitch {
    Midi(u8),
    Name(String),
}

impl Pitch {
    pub fn midi(&self) -> Result<u8, String> {
        match self {
            Pitch::Midi(n) => Ok(*n),
            Pitch::Name(name) => parse_note(name).ok_or_else(|| format!("Note illisible : « {name} »")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct PatternUse {
    pub pattern: String,
    /// Premier départ, en temps.
    pub at: f32,
    #[serde(default)]
    pub transpose: i32,
    /// Nombre de répétitions.
    #[serde(default = "one")]
    pub times: u32,
    /// Écart entre deux répétitions, en temps (défaut : la longueur du motif arrondie à la mesure).
    #[serde(default)]
    pub every: Option<f32>,
    /// Multiplie la force des notes du motif.
    #[serde(default = "unit")]
    pub vel: f32,
}

fn one() -> u32 {
    1
}
fn unit() -> f32 {
    1.0
}

/// Un stem rendu : son nom et ses deux canaux, de la longueur exacte du morceau.
#[derive(Debug, Clone)]
pub struct Stem {
    pub name: String,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Music {
    pub fn seconds_per_beat(&self) -> f32 {
        60.0 / self.bpm
    }

    /// Durée de la boucle, en secondes.
    pub fn duration(&self) -> f32 {
        (self.bars * self.beats_per_bar) as f32 * self.seconds_per_beat()
    }

    /// Longueur exacte de chaque stem, en échantillons.
    pub fn length(&self) -> usize {
        (self.duration() * SAMPLE_RATE as f32).round() as usize
    }

    pub fn check(&self) -> Result<(), String> {
        if !(20.0..=300.0).contains(&self.bpm) {
            return Err("Tempo entre 20 et 300 battements par minute.".into());
        }
        if self.bars == 0 || self.beats_per_bar == 0 || self.duration() > MAX_SECONDS {
            return Err(format!("Le morceau doit durer entre une mesure et {MAX_SECONDS} s."));
        }
        if self.tracks.is_empty() {
            return Err("Il faut au moins une piste.".into());
        }
        let count: usize = self.tracks.iter().map(|t| self.notes_of(t).map_or(0, |n| n.len())).sum();
        if count > MAX_NOTES {
            return Err(format!("{count} notes : {MAX_NOTES} au plus."));
        }
        for track in &self.tracks {
            self.notes_of(track)?;
        }
        Ok(())
    }

    /// Toutes les notes d'une piste, motifs déroulés, en (début en temps, durée en temps, note MIDI, force).
    pub fn notes_of(&self, track: &Track) -> Result<Vec<(f32, f32, u8, f32)>, String> {
        let mut out = Vec::new();
        let shift = |n: u8, t: i32| -> Result<u8, String> {
            u8::try_from(i32::from(n) + t + track.transpose)
                .ok()
                .filter(|n| *n <= 127)
                .ok_or_else(|| format!("Note hors de portée dans la piste « {} ».", track.name))
        };
        for note in &track.notes {
            out.push((note.at, note.len, shift(note.note.midi()?, 0)?, note.vel));
        }
        for usage in &track.play {
            let pattern = self
                .patterns
                .get(&usage.pattern)
                .ok_or_else(|| format!("Motif inconnu « {} » (piste « {} »).", usage.pattern, track.name))?;
            let span = pattern.iter().map(|n| n.at + n.len).fold(0.0, f32::max);
            let bar = self.beats_per_bar as f32;
            let every = usage.every.unwrap_or(((span / bar).ceil() * bar).max(bar));
            for k in 0..usage.times {
                for note in pattern {
                    out.push((
                        usage.at + k as f32 * every + note.at,
                        note.len,
                        shift(note.note.midi()?, usage.transpose)?,
                        (note.vel * usage.vel).clamp(0.0, 1.0),
                    ));
                }
            }
        }
        Ok(out)
    }
}

/// Rend les stems du morceau. `instruments` : le dossier du catalogue. Même graine = mêmes fichiers.
pub fn render_music(music: &Music, instruments: &Path, seed: u64) -> Result<Vec<Stem>, String> {
    music.check()?;
    let catalogue = catalogue_for(music, instruments)?;
    let tracks = music
        .tracks
        .iter()
        .map(|track| render_track(music, track, &catalogue, instruments, seed))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(assemble(music, &tracks))
}

/// Le catalogue, seulement si une piste joue un instrument échantillonné.
pub fn catalogue_for(music: &Music, instruments: &Path) -> Result<Catalogue, String> {
    if music.tracks.iter().any(|t| matches!(t.instrument, Voice::Sampled(_))) {
        Catalogue::load(instruments)
    } else {
        Ok(Catalogue::default())
    }
}

/// Longueur d'une piste rendue seule : la boucle et la place de sa queue.
fn rendered_length(music: &Music) -> usize {
    music.length() + (MAX_TAIL * SAMPLE_RATE as f32) as usize
}

/// Graine propre à une piste : retoucher une piste ne change pas le jeu humain des autres, ce qui permet
/// à un éditeur de ne refaire que la piste modifiée.
pub fn track_seed(seed: u64, track: &Track) -> u64 {
    track.name.bytes().fold(0xcbf2_9ce4_8422_2325_u64 ^ seed, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Place les pistes rendues (mono, dans l'ordre de `music.tracks`) dans leurs stems : panoramique, espace commun,
/// queue cyclique, un seul gain pour tous les stems.
pub fn assemble(music: &Music, tracks: &[Vec<f32>]) -> Vec<Stem> {
    let rate = SAMPLE_RATE as f32;
    let length = music.length();
    let total = rendered_length(music);
    let mut order: Vec<String> = Vec::new();
    let mut stems: HashMap<String, (Vec<f32>, Vec<f32>, Vec<f32>)> = HashMap::new(); // gauche, droite, envoi réverb

    for (track, mono) in music.tracks.iter().zip(tracks) {
        let stem = track.stem.clone().unwrap_or_else(|| track.name.clone());
        if !stems.contains_key(&stem) {
            order.push(stem.clone());
            stems.insert(stem.clone(), (vec![0.0; total], vec![0.0; total], vec![0.0; total]));
        }
        let (left, right, send) = stems.get_mut(&stem).unwrap();
        // Panoramique à puissance constante.
        let angle = (track.pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
        let (gl, gr) = (angle.cos(), angle.sin());
        let send_gain = track.reverb.clamp(0.0, 1.0);
        for (i, s) in mono.iter().enumerate().take(total) {
            left[i] += s * gl;
            right[i] += s * gr;
            send[i] += s * send_gain;
        }
    }

    let mut out: Vec<Stem> = order
        .into_iter()
        .map(|name| {
            let (mut left, mut right, send) = stems.remove(&name).unwrap();
            if let Some(reverb) = music.reverb {
                reverb_stereo(&send, &mut left, &mut right, reverb, rate);
            }
            if music.looping {
                // Queue cyclique : ce qui dépasse la boucle revient sur son début.
                for i in length..total {
                    left[i - length] += left[i];
                    right[i - length] += right[i];
                }
                left.truncate(length);
                right.truncate(length);
            } else {
                let end = last_audible(&left, &right).max(length).min(total);
                left.truncate(end);
                right.truncate(end);
            }
            Stem { name, left, right }
        })
        .collect();

    // Un seul gain pour tous les stems, calculé sur leur somme : l'équilibre voulu reste.
    let len = out.iter().map(|s| s.left.len()).max().unwrap_or(0);
    for stem in &mut out {
        stem.left.resize(len, 0.0);
        stem.right.resize(len, 0.0);
    }
    let peak = (0..len)
        .map(|i| {
            let l: f32 = out.iter().map(|s| s.left[i]).sum();
            let r: f32 = out.iter().map(|s| s.right[i]).sum();
            l.abs().max(r.abs())
        })
        .fold(0.0, f32::max);
    if peak > f32::EPSILON {
        let gain = 10_f32.powf(music.peak_db.min(0.0) / 20.0) / peak;
        for stem in &mut out {
            stem.left.iter_mut().chain(stem.right.iter_mut()).for_each(|s| *s *= gain);
        }
    }
    out
}

/// La somme des stems : l'écoute d'ensemble.
pub fn mixdown(stems: &[Stem]) -> Stem {
    let len = stems.iter().map(|s| s.left.len()).max().unwrap_or(0);
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];
    for stem in stems {
        for i in 0..stem.left.len() {
            left[i] += stem.left[i];
            right[i] += stem.right[i];
        }
    }
    Stem { name: "mix".into(), left, right }
}

fn last_audible(left: &[f32], right: &[f32]) -> usize {
    let peak = left.iter().chain(right).fold(0.0_f32, |m, s| m.max(s.abs()));
    let threshold = peak * 0.0005;
    left.iter()
        .zip(right)
        .rposition(|(l, r)| l.abs() > threshold || r.abs() > threshold)
        .map_or(0, |i| i + (0.05 * SAMPLE_RATE as f32) as usize)
}

/// Une piste rendue seule (mono, longueur boucle + queue), avant placement dans son stem.
pub fn render_track(
    music: &Music,
    track: &Track,
    catalogue: &Catalogue,
    instruments: &Path,
    seed: u64,
) -> Result<Vec<f32>, String> {
    let rate = SAMPLE_RATE as f32;
    let total = rendered_length(music);
    let rng = &mut Rng::new(track_seed(seed, track));
    let beat = music.seconds_per_beat();
    let mut out = vec![0.0_f32; total];
    let gain = 10_f32.powf(track.volume_db / 20.0);
    let humanize = track.humanize.clamp(0.0, 1.0);
    for (at, len, note, vel) in music.notes_of(track)? {
        // Jeu humain : ±12 ms et ±8 % de force au plus, tirés de la graine (rendu reproductible).
        let jitter = rng.bipolar() * humanize * 0.012;
        let vel = (vel * (1.0 + rng.bipolar() * humanize * 0.08)).clamp(0.02, 1.0);
        let start = ((at * beat + jitter).max(0.0) * rate) as usize;
        let seconds = (len * beat).max(0.02);
        let pick = rng.unit();
        let voice = match &track.instrument {
            Voice::Sampled(id) => {
                let instrument = catalogue
                    .get(id)
                    .ok_or_else(|| format!("Instrument inconnu « {id} » (piste « {} ») : voir list_instruments.", track.name))?;
                if note < instrument.range.0 || note > instrument.range.1 {
                    return Err(format!(
                        "Piste « {} » : note {note} hors de la tessiture de « {id} » ({}–{}).",
                        track.name, instrument.range.0, instrument.range.1
                    ));
                }
                let (zone, shift) = instrument.zone_for(note, vel, pick).ok_or("Instrument sans enregistrement.")?;
                let data = sample::load(&instruments.join(&zone.file))?;
                let speed = 2_f32.powf(shift as f32 / 12.0);
                let level = 10_f32.powf((instrument.gain_db + instrument.level_db) / 20.0) * vel.powf(1.4);
                let length = match instrument.kind {
                    Kind::Tenu => seconds,
                    Kind::Frappe => MAX_ONE_SHOT,
                };
                play_sample(&data, speed, length, if instrument.kind == Kind::Tenu { instrument.release } else { 0.05 }, level)
            }
            Voice::Synth { synth } => synth_note(synth, note, seconds, vel, rng.next_u64()),
        };
        for (i, s) in voice.iter().enumerate() {
            if let Some(o) = out.get_mut(start + i) {
                *o += s * gain;
            }
        }
    }
    Ok(out)
}

/// Lit un enregistrement à une vitesse donnée pendant `seconds`, puis l'éteint en `release` secondes.
fn play_sample(data: &[f32], speed: f32, seconds: f32, release: f32, level: f32) -> Vec<f32> {
    let rate = SAMPLE_RATE as f32;
    let available = (data.len() as f32 / speed) as usize;
    let hold = ((seconds * rate) as usize).min(available);
    let fade = ((release.max(0.005) * rate) as usize).min(available.saturating_sub(hold));
    let count = hold + fade;
    let attack = (0.002 * rate) as usize;
    (0..count)
        .map(|i| {
            let position = i as f32 * speed;
            let index = position as usize;
            let fraction = position - index as f32;
            let a = data.get(index).copied().unwrap_or(0.0);
            let b = data.get(index + 1).copied().unwrap_or(0.0);
            let mut envelope = if i < attack { i as f32 / attack as f32 } else { 1.0 };
            if i >= hold {
                let r = 1.0 - (i - hold) as f32 / fade.max(1) as f32;
                envelope *= r * r;
            }
            (a + (b - a) * fraction) * envelope * level
        })
        .collect()
}

/// Un son de synthèse joué comme un instrument : transposé pour que sa première couche donne la note, tenu
/// le temps de la note s'il a un palier.
fn synth_note(sound: &Sound, note: u8, seconds: f32, vel: f32, seed: u64) -> Vec<f32> {
    let target = 440.0 * 2_f32.powf((f32::from(note) - 69.0) / 12.0);
    let reference = sound.layers.first().map_or(440.0, |l| l.pitch.start.max(1.0));
    let ratio = target / reference;
    let mut sound = sound.clone();
    for layer in &mut sound.layers {
        layer.pitch = layer.pitch.scaled(ratio);
        let e = &mut layer.envelope;
        if e.sustain > 0.0 {
            e.hold = (seconds - layer.start - e.attack - e.decay).max(0.0);
        }
    }
    let mut samples = render(&sound, seed);
    // Même calibrage que les instruments échantillonnés : le corps de la note (ce qui dépasse -40 dB sous la
    // crête) est ramené au niveau de référence.
    let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    let body: Vec<f32> = samples.iter().copied().filter(|s| s.abs() > peak * 0.01).collect();
    let rms = (body.iter().map(|s| s * s).sum::<f32>() / body.len().max(1) as f32).sqrt();
    let level = if rms > 1e-6 { 10_f32.powf(REFERENCE_DB / 20.0) / rms } else { 0.0 } * vel.powf(1.4);
    samples.iter_mut().for_each(|s| *s *= level);
    samples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Adsr, Sweep};
    use crate::oscillator::Waveform;
    use crate::sound::Layer;

    fn pad() -> Voice {
        Voice::Synth {
            synth: Sound::new("pad")
                .layer(Layer::new(Waveform::Sine, Sweep::constant(440.0), Adsr::new(0.01, 0.05, 0.8, 0.0, 0.2)))
                .peak_db(-6.0),
        }
    }

    fn note(at: f32, len: f32, name: &str) -> Note {
        Note { at, len, note: Pitch::Name(name.into()), vel: 0.8 }
    }

    fn music() -> Music {
        Music {
            name: "essai".into(),
            bpm: 120.0,
            beats_per_bar: 4,
            bars: 2,
            looping: true,
            patterns: BTreeMap::from([("motif".into(), vec![note(0.0, 1.0, "D4"), note(1.0, 1.0, "F4")])]),
            tracks: vec![
                Track { name: "nappe".into(), stem: Some("ambience".into()), instrument: pad(), volume_db: 0.0, pan: -0.5, reverb: 0.3, transpose: 0, humanize: 0.0, notes: vec![note(6.0, 4.0, "A3")], play: vec![] },
                Track { name: "chant".into(), stem: Some("melody".into()), instrument: pad(), volume_db: -3.0, pan: 0.5, reverb: 0.2, transpose: 12, humanize: 0.3, notes: vec![], play: vec![PatternUse { pattern: "motif".into(), at: 0.0, transpose: 0, times: 2, every: None, vel: 1.0 }] },
            ],
            reverb: Some(Reverb { room: 0.5, damping: 0.5, mix: 0.3 }),
            peak_db: -1.0,
        }
    }

    #[test]
    fn stems_share_the_exact_loop_length_and_one_gain() {
        let m = music();
        let stems = render_music(&m, Path::new("/nulle/part"), 3).unwrap();
        assert_eq!(stems.len(), 2);
        assert_eq!(m.length(), 4 * 44_100);
        assert!(stems.iter().all(|s| s.left.len() == m.length() && s.right.len() == m.length()));
        let mix = mixdown(&stems);
        let peak = mix.left.iter().chain(&mix.right).fold(0.0_f32, |p, s| p.max(s.abs()));
        assert!((20.0 * peak.log10() + 1.0).abs() < 0.05, "crête du mélange {}", 20.0 * peak.log10());
    }

    #[test]
    fn the_tail_of_a_loop_comes_back_on_its_start() {
        // La nappe commence au temps 6 (mesure 2) et dure 4 temps : elle déborde de la boucle de 2 temps.
        let mut m = music();
        m.tracks.truncate(1);
        m.reverb = None;
        let stems = render_music(&m, Path::new("/nulle/part"), 1).unwrap();
        let early: f32 = stems[0].left[..(0.5 * 44_100.0) as usize].iter().map(|s| s * s).sum();
        assert!(early > 0.0, "le débordement doit s'entendre au début de la boucle");
        m.looping = false;
        let stems = render_music(&m, Path::new("/nulle/part"), 1).unwrap();
        assert!(stems[0].left.len() > m.length(), "sans boucle, la queue se prolonge après la dernière mesure");
    }

    #[test]
    fn patterns_unroll_and_notes_are_checked() {
        let m = music();
        let notes = m.notes_of(&m.tracks[1]).unwrap();
        assert_eq!(notes.len(), 4);
        assert_eq!(notes[2].0, 4.0); // la répétition tombe à la mesure suivante
        assert_eq!(notes[0].2, 62 + 12);
        let mut bad = music();
        bad.tracks[0].notes[0].note = Pitch::Name("H9".into());
        assert!(bad.check().is_err());
        assert_eq!(render_music(&m, Path::new("/x"), 5).unwrap()[0].left, render_music(&m, Path::new("/x"), 5).unwrap()[0].left);
    }
}
