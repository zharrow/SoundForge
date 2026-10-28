//! Description d'un effet sonore : des couches superposées, puis des effets.
//!
//! Un SFX moderne se construit en couches : une attaque courte qui donne
//! l'impact, un corps qui donne la hauteur, une queue qui donne l'espace.

use crate::effects::{Echo, Effects, Reverb};
use crate::envelope::{Adsr, Sweep};
use crate::filter::FilterMode;
use crate::oscillator::Waveform;
use crate::resonator::Resonator;

const MAX_LAYERS: usize = 16;
const MAX_DURATION: f32 = 10.0;
const MAX_VOICES: u8 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct FilterSpec {
    pub mode: FilterMode,
    /// Fréquence de coupure en Hz, éventuellement balayée dans le temps.
    pub cutoff: Sweep,
    /// De 0 (doux) à 0.95 (sifflant). 0.3 est neutre.
    pub resonance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Vibrato {
    /// Vitesse d'oscillation de la hauteur, en Hz (5 à 7 = vibrato naturel).
    pub rate_hz: f32,
    /// Amplitude en demi-tons (0.1 à 0.5 en général).
    pub depth_semitones: f32,
}

/// Plusieurs oscillateurs légèrement désaccordés : rend un son large et riche.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Unison {
    /// Nombre de voix, de 1 à 16.
    pub voices: u8,
    /// Écart total entre les voix extrêmes, en centièmes de demi-ton (10 à 25 pour un supersaw).
    pub detune_cents: f32,
}

impl Default for Unison {
    fn default() -> Self {
        Self {
            voices: 1,
            detune_cents: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Layer {
    /// Rôle de la couche (« attaque », « corps », « queue »…), pour s'y retrouver. Sans effet sur le son.
    #[cfg_attr(feature = "schema", serde(default, skip_serializing_if = "Option::is_none"))]
    pub name: Option<String>,
    pub waveform: Waveform,
    /// Hauteur en Hz. Ignorée par les bruits : pour eux, c'est le filtre qui donne la couleur.
    pub pitch: Sweep,
    pub envelope: Adsr,
    pub filter: Option<FilterSpec>,
    pub vibrato: Option<Vibrato>,
    #[cfg_attr(feature = "schema", serde(default))]
    pub unison: Unison,
    /// Volume relatif de la couche dans le mixage (1 = référence).
    #[cfg_attr(feature = "schema", serde(default = "default_gain"))]
    pub gain: f32,
    /// Décalage de départ de la couche, en secondes.
    #[cfg_attr(feature = "schema", serde(default))]
    pub start: f32,
    /// Objet qui résonne sous la couche (synthèse modale). La couche devient alors le COUP
    /// et sa hauteur donne la fondamentale de l'objet.
    #[cfg_attr(feature = "schema", serde(default))]
    pub resonator: Option<Resonator>,
}

fn default_gain() -> f32 {
    1.0
}

impl Layer {
    pub fn new(waveform: Waveform, pitch: Sweep, envelope: Adsr) -> Self {
        Self {
            name: None,
            waveform,
            pitch,
            envelope,
            filter: None,
            vibrato: None,
            unison: Unison::default(),
            gain: default_gain(),
            start: 0.0,
            resonator: None,
        }
    }

    pub fn filter(mut self, mode: FilterMode, cutoff: Sweep, resonance: f32) -> Self {
        self.filter = Some(FilterSpec {
            mode,
            cutoff,
            resonance,
        });
        self
    }

    pub fn vibrato(mut self, rate_hz: f32, depth_semitones: f32) -> Self {
        self.vibrato = Some(Vibrato {
            rate_hz,
            depth_semitones,
        });
        self
    }

    pub fn unison(mut self, voices: u8, detune_cents: f32) -> Self {
        self.unison = Unison {
            voices: voices.max(1),
            detune_cents,
        };
        self
    }

    pub fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    pub fn resonator(mut self, resonator: Resonator) -> Self {
        self.resonator = Some(resonator);
        self
    }

    pub fn start_at(mut self, seconds: f32) -> Self {
        self.start = seconds.max(0.0);
        self
    }

    /// Fin de la couche, queue du résonateur comprise.
    pub fn end_time(&self) -> f32 {
        self.start + self.envelope.duration() + self.resonator.as_ref().map_or(0.0, Resonator::tail)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Sound {
    pub name: String,
    /// Couches mixées ensemble (16 maximum).
    pub layers: Vec<Layer>,
    #[cfg_attr(feature = "schema", serde(default))]
    pub effects: Effects,
    /// Niveau de crête visé après normalisation, en dBFS (≤ 0). Règle le volume relatif entre SFX :
    /// -1 pour un impact fort, -6 à -13 pour l'interface.
    #[cfg_attr(feature = "schema", serde(default = "default_peak_db"))]
    pub peak_db: f32,
}

fn default_peak_db() -> f32 {
    -1.0
}

impl Sound {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            layers: Vec::new(),
            effects: Effects::default(),
            peak_db: default_peak_db(),
        }
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layers.push(layer);
        self
    }

    pub fn drive(mut self, amount: f32) -> Self {
        self.effects.drive = Some(amount);
        self
    }

    pub fn echo(mut self, time: f32, feedback: f32, mix: f32) -> Self {
        self.effects.echo = Some(Echo {
            time,
            feedback,
            mix,
        });
        self
    }

    pub fn reverb(mut self, room: f32, damping: f32, mix: f32) -> Self {
        self.effects.reverb = Some(Reverb { room, damping, mix });
        self
    }

    pub fn peak_db(mut self, db: f32) -> Self {
        self.peak_db = db.min(0.0);
        self
    }

    /// Durée des couches seules, sans la queue des effets.
    pub fn duration(&self) -> f32 {
        self.layers.iter().map(Layer::end_time).fold(0.0, f32::max)
    }

    /// Vérifie qu'un son reçu de l'extérieur (JSON, MCP) est rendable sans risque.
    pub fn check(&self) -> Result<(), String> {
        if self.layers.is_empty() || self.layers.len() > MAX_LAYERS {
            return Err(format!("Il faut entre 1 et {MAX_LAYERS} couches."));
        }
        if !(self.duration() > 0.0 && self.duration() <= MAX_DURATION) {
            return Err(format!(
                "La durée doit être comprise entre 0 et {MAX_DURATION} s."
            ));
        }
        if self.layers.iter().any(|l| l.unison.voices > MAX_VOICES) {
            return Err(format!("{MAX_VOICES} voix d'unisson maximum par couche."));
        }
        if !self.layers.iter().all(Layer::is_finite) || !self.peak_db.is_finite() {
            return Err("Valeur numérique invalide (NaN ou infini).".into());
        }
        if self
            .layers
            .iter()
            .any(|l| l.start < 0.0 || l.envelope.duration() < 0.0)
        {
            return Err("Les durées et décalages doivent être positifs.".into());
        }
        for resonator in self.layers.iter().filter_map(|l| l.resonator.as_ref()) {
            resonator.check()?;
        }
        Ok(())
    }
}

impl Layer {
    fn is_finite(&self) -> bool {
        let env = &self.envelope;
        [
            self.pitch.start,
            self.pitch.end,
            self.pitch.time,
            env.attack,
            env.decay,
            env.sustain,
            env.hold,
            env.release,
            self.gain,
            self.start,
            self.unison.detune_cents,
        ]
        .iter()
        .all(|v| v.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone() -> Layer {
        Layer::new(
            Waveform::Sine,
            Sweep::constant(440.0),
            Adsr::percussive(0.01, 0.2),
        )
    }

    #[test]
    fn check_accepts_presets_shape_and_rejects_bad_input() {
        assert!(Sound::new("ok").layer(tone()).check().is_ok());
        assert!(Sound::new("vide").check().is_err());
        let too_long = Layer::new(
            Waveform::Sine,
            Sweep::constant(440.0),
            Adsr::percussive(0.0, 60.0),
        );
        assert!(Sound::new("long").layer(too_long).check().is_err());
        let nan = tone().gain(f32::NAN);
        assert!(Sound::new("nan").layer(nan).check().is_err());
        assert!(
            Sound::new("voix")
                .layer(tone().unison(40, 10.0))
                .check()
                .is_err()
        );
    }
}
