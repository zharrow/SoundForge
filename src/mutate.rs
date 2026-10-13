//! Variantes d'un son : un même SFX joué en boucle fatigue l'oreille,
//! quelques versions légèrement différentes le rendent naturel.

use crate::rng::Rng;
use crate::sound::{Layer, Sound};

/// Écart de hauteur maximal (demi-tons) quand `amount` vaut 1.
const MAX_PITCH_SEMITONES: f32 = 2.0;
const MAX_TIME_SPREAD: f32 = 0.25;
const MAX_GAIN_SPREAD: f32 = 0.15;
const MAX_CUTOFF_SPREAD: f32 = 0.3;

/// Crée une variante. `amount` entre 0 (identique) et 1 (très différente) ; 0.3 à 0.5 est un bon réglage.
/// La hauteur bouge d'un bloc pour toutes les couches, pour garder l'harmonie entre elles.
pub fn vary(sound: &Sound, amount: f32, rng: &mut Rng) -> Sound {
    let amount = amount.clamp(0.0, 1.0);
    let pitch_ratio = 2_f32.powf(rng.bipolar() * amount * MAX_PITCH_SEMITONES / 12.0);
    let time_ratio = 1.0 + rng.bipolar() * amount * MAX_TIME_SPREAD;

    let mut variant = sound.clone();
    for layer in &mut variant.layers {
        vary_layer(layer, pitch_ratio, time_ratio, amount, rng);
    }
    variant
}

fn vary_layer(layer: &mut Layer, pitch_ratio: f32, time_ratio: f32, amount: f32, rng: &mut Rng) {
    layer.pitch = layer.pitch.scaled(pitch_ratio).stretched(time_ratio);
    layer.envelope = layer.envelope.stretched(time_ratio);
    layer.start *= time_ratio;
    layer.gain *= 1.0 + rng.bipolar() * amount * MAX_GAIN_SPREAD;
    if let Some(resonator) = &mut layer.resonator {
        // La hauteur suit déjà `pitch` ; l'objet résonne un peu plus ou moins longtemps.
        resonator.decay *= time_ratio;
    }
    if let Some(filter) = &mut layer.filter {
        filter.cutoff = filter
            .cutoff
            .scaled(1.0 + rng.bipolar() * amount * MAX_CUTOFF_SPREAD);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Adsr, Sweep};
    use crate::oscillator::Waveform;

    fn sound() -> Sound {
        Sound::new("test").layer(Layer::new(
            Waveform::Saw,
            Sweep::constant(220.0),
            Adsr::percussive(0.0, 0.2),
        ))
    }

    #[test]
    fn zero_amount_keeps_the_sound() {
        assert_eq!(vary(&sound(), 0.0, &mut Rng::new(3)), sound());
    }

    #[test]
    fn variant_pitch_stays_within_two_semitones() {
        let mut rng = Rng::new(5);
        for _ in 0..100 {
            let pitch = vary(&sound(), 1.0, &mut rng).layers[0].pitch.start;
            let semitones = 12.0 * (pitch / 220.0).log2();
            assert!(semitones.abs() <= MAX_PITCH_SEMITONES + 1e-3);
        }
    }
}
