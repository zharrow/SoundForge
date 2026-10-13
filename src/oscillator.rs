//! Oscillateurs anti-crénelés (PolyBLEP) et générateurs de bruit.

use std::f32::consts::TAU;

use crate::rng::Rng;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
#[cfg_attr(feature = "schema", serde(rename_all = "snake_case"))]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    /// `duty` : rapport cyclique, 0.5 = carré parfait.
    Square {
        duty: f32,
    },
    WhiteNoise,
    /// Bruit grave et sourd, idéal pour les grondements et explosions.
    BrownNoise,
}

#[derive(Debug, Clone)]
pub struct Oscillator {
    waveform: Waveform,
    phase: f32,
    brown_state: f32,
}

impl Oscillator {
    pub fn new(waveform: Waveform, phase: f32) -> Self {
        Self {
            waveform,
            phase: phase.fract(),
            brown_state: 0.0,
        }
    }

    /// Produit l'échantillon suivant à la fréquence `freq` (Hz).
    pub fn next(&mut self, freq: f32, sample_rate: f32, rng: &mut Rng) -> f32 {
        let step = (freq / sample_rate).clamp(0.0, 0.5);
        let value = self.sample(step, rng);
        self.phase = (self.phase + step).fract();
        value
    }

    fn sample(&mut self, step: f32, rng: &mut Rng) -> f32 {
        let phase = self.phase;
        match self.waveform {
            Waveform::Sine => (phase * TAU).sin(),
            Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            Waveform::Saw => 2.0 * phase - 1.0 - poly_blep(phase, step),
            Waveform::Square { duty } => square(phase, duty, step),
            Waveform::WhiteNoise => rng.bipolar(),
            Waveform::BrownNoise => self.next_brown(rng),
        }
    }

    fn next_brown(&mut self, rng: &mut Rng) -> f32 {
        self.brown_state = self.brown_state * 0.995 + rng.bipolar() * 0.05;
        (self.brown_state * 3.0).clamp(-1.0, 1.0)
    }
}

fn square(phase: f32, duty: f32, step: f32) -> f32 {
    let duty = duty.clamp(0.05, 0.95);
    let naive = if phase < duty { 1.0 } else { -1.0 };
    naive + poly_blep(phase, step) - poly_blep((phase - duty + 1.0).fract(), step)
}

/// Correction PolyBLEP : adoucit les discontinuités pour supprimer le crénelage.
fn poly_blep(phase: f32, step: f32) -> f32 {
    if step <= 0.0 {
        return 0.0;
    }
    if phase < step {
        let x = phase / step;
        return 2.0 * x - x * x - 1.0;
    }
    if phase > 1.0 - step {
        let x = (phase - 1.0) / step;
        return x * x + 2.0 * x + 1.0;
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(waveform: Waveform) -> Vec<f32> {
        let mut osc = Oscillator::new(waveform, 0.0);
        let mut rng = Rng::new(1);
        (0..4410)
            .map(|_| osc.next(440.0, 44_100.0, &mut rng))
            .collect()
    }

    #[test]
    fn waveforms_stay_bounded() {
        let all = [
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Saw,
            Waveform::Square { duty: 0.3 },
            Waveform::WhiteNoise,
            Waveform::BrownNoise,
        ];
        for waveform in all {
            assert!(
                render(waveform).iter().all(|s| s.abs() <= 1.1),
                "{waveform:?}"
            );
        }
    }

    #[test]
    fn sine_has_no_dc_offset() {
        let mean: f32 = render(Waveform::Sine).iter().sum::<f32>() / 4410.0;
        assert!(mean.abs() < 0.01);
    }
}
