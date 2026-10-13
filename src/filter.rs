//! Filtre à variables d'état (SVF, topologie TPT).
//!
//! Reste stable quand la fréquence de coupure bouge à chaque échantillon,
//! ce qui permet les balayages de filtre typiques des sons modernes.

use std::f32::consts::PI;

/// low_pass garde les graves, high_pass garde les aigus, band_pass garde une bande autour de la coupure.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
#[cfg_attr(feature = "schema", serde(rename_all = "snake_case"))]
pub enum FilterMode {
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// `resonance` entre 0 (doux) et ~0.95 (sifflant). 0.3 est proche d'un Butterworth.
    pub fn process(
        &mut self,
        input: f32,
        cutoff: f32,
        resonance: f32,
        mode: FilterMode,
        sample_rate: f32,
    ) -> f32 {
        let cutoff = cutoff.clamp(20.0, sample_rate * 0.45);
        let g = (PI * cutoff / sample_rate).tan();
        let k = 2.0 * (1.0 - resonance.clamp(0.0, 0.97));
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;

        match mode {
            FilterMode::LowPass => v2,
            FilterMode::BandPass => v1,
            FilterMode::HighPass => input - k * v1 - v2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amplitude_after_filter(freq: f32, mode: FilterMode) -> f32 {
        let mut filter = Svf::default();
        let sr = 44_100.0;
        (0..8820)
            .map(|i| (2.0 * PI * freq * i as f32 / sr).sin())
            .map(|x| filter.process(x, 1000.0, 0.3, mode, sr))
            .skip(4410)
            .fold(0.0_f32, |max, s| max.max(s.abs()))
    }

    #[test]
    fn lowpass_keeps_lows_and_cuts_highs() {
        assert!(amplitude_after_filter(100.0, FilterMode::LowPass) > 0.9);
        assert!(amplitude_after_filter(10_000.0, FilterMode::LowPass) < 0.05);
    }

    #[test]
    fn highpass_cuts_lows() {
        assert!(amplitude_after_filter(100.0, FilterMode::HighPass) < 0.05);
        assert!(amplitude_after_filter(10_000.0, FilterMode::HighPass) > 0.9);
    }
}
