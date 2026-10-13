//! Répartition de l'énergie par bandes de fréquences, et détection de masquage.
//!
//! Deux sons qui concentrent leur énergie dans les mêmes bandes se masquent :
//! l'un « mange » l'autre au mixage. C'est ce qu'on vérifie entre un SFX et la musique.

use std::fmt;

use crate::fft::{hann, power_spectrum};

pub const BANDS: [Band; 7] = [
    Band {
        name: "sub",
        low: 20.0,
        high: 60.0,
    },
    Band {
        name: "basses",
        low: 60.0,
        high: 250.0,
    },
    Band {
        name: "bas-médiums",
        low: 250.0,
        high: 500.0,
    },
    Band {
        name: "médiums",
        low: 500.0,
        high: 2000.0,
    },
    Band {
        name: "haut-médiums",
        low: 2000.0,
        high: 4000.0,
    },
    Band {
        name: "présence",
        low: 4000.0,
        high: 6000.0,
    },
    Band {
        name: "air",
        low: 6000.0,
        high: 20000.0,
    },
];

const FRAME_SIZE: usize = 2048;
/// Au-delà de cette part d'énergie dans une même bande, deux sons entrent en conflit.
const CONFLICT_SHARE: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub name: &'static str,
    pub low: f32,
    pub high: f32,
}

/// Part de l'énergie totale dans chaque bande de `BANDS` (la somme vaut 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandProfile(pub [f32; BANDS.len()]);

impl BandProfile {
    pub fn dominant(&self) -> &'static str {
        let index = (0..BANDS.len())
            .max_by(|&a, &b| self.0[a].total_cmp(&self.0[b]))
            .unwrap_or(0);
        BANDS[index].name
    }
}

impl fmt::Display for BandProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (band, share) in BANDS.iter().zip(self.0) {
            let bar = "█".repeat((share * 40.0).round() as usize);
            writeln!(
                f,
                "{:<13} {:>5}-{:<5} Hz {:>5.1} % {bar}",
                band.name,
                band.low,
                band.high,
                share * 100.0
            )?;
        }
        Ok(())
    }
}

/// Spectre moyen sur tout le son (méthode de Welch, trames recouvrantes à 50 %).
pub fn band_profile(samples: &[f32], sample_rate: u32) -> BandProfile {
    let window = hann(FRAME_SIZE);
    let mut average = vec![0.0_f32; FRAME_SIZE / 2];
    for frame in frames(samples) {
        for (sum, power) in average.iter_mut().zip(power_spectrum(frame, &window)) {
            *sum += power;
        }
    }
    let bin_hz = sample_rate as f32 / FRAME_SIZE as f32;
    let energies = BANDS.map(|band| band_energy(&average, bin_hz, band));
    normalize(energies)
}

fn frames(samples: &[f32]) -> impl Iterator<Item = &[f32]> {
    let hop = FRAME_SIZE / 2;
    let count = samples.len().saturating_sub(1) / hop + 1;
    (0..count).map(move |i| {
        let start = i * hop;
        &samples[start..(start + FRAME_SIZE).min(samples.len())]
    })
}

fn band_energy(spectrum: &[f32], bin_hz: f32, band: Band) -> f32 {
    spectrum
        .iter()
        .enumerate()
        .filter(|(bin, _)| {
            let freq = *bin as f32 * bin_hz;
            freq >= band.low && freq < band.high
        })
        .map(|(_, power)| power)
        .sum()
}

fn normalize(energies: [f32; BANDS.len()]) -> BandProfile {
    let total: f32 = energies.iter().sum();
    if total <= f32::EPSILON {
        return BandProfile([0.0; BANDS.len()]);
    }
    BandProfile(energies.map(|e| e / total))
}

/// Résultat de la comparaison de deux sons.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlap {
    /// 0 = aucune bande commune, 1 = répartitions identiques.
    pub score: f32,
    /// Bandes où les deux sons dépassent chacun `CONFLICT_SHARE` de leur énergie.
    pub conflicts: Vec<&'static str>,
}

pub fn overlap(a: &BandProfile, b: &BandProfile) -> Overlap {
    let score = a.0.iter().zip(&b.0).map(|(x, y)| x.min(*y)).sum();
    let conflicts = BANDS
        .iter()
        .zip(a.0.iter().zip(&b.0))
        .filter(|(_, (x, y))| **x > CONFLICT_SHARE && **y > CONFLICT_SHARE)
        .map(|(band, _)| band.name)
        .collect();
    Overlap { score, conflicts }
}

impl fmt::Display for Overlap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.score {
            s if s < 0.3 => "faible : les deux sons cohabitent bien",
            s if s < 0.6 => "moyen : surveiller au mixage",
            _ => "fort : risque de masquage, séparer les fréquences",
        };
        writeln!(f, "Recouvrement {:.0} % ({level})", self.score * 100.0)?;
        if self.conflicts.is_empty() {
            return writeln!(f, "Aucune bande en conflit.");
        }
        writeln!(f, "Bandes en conflit : {}", self.conflicts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32) -> Vec<f32> {
        (0..44_100)
            .map(|i| (2.0 * PI * freq * i as f32 / 44_100.0).sin())
            .collect()
    }

    #[test]
    fn sine_energy_lands_in_its_band() {
        let profile = band_profile(&sine(1000.0), 44_100);
        assert_eq!(profile.dominant(), "médiums");
        assert!(profile.0[3] > 0.95);
        assert!((profile.0.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn overlap_detects_conflict_and_separation() {
        let low = band_profile(&sine(100.0), 44_100);
        let high = band_profile(&sine(8000.0), 44_100);
        let same = overlap(&low, &low);
        assert!(same.score > 0.99 && same.conflicts == ["basses"]);
        let apart = overlap(&low, &high);
        assert!(apart.score < 0.05 && apart.conflicts.is_empty());
    }

    #[test]
    fn silence_gives_empty_profile() {
        assert_eq!(band_profile(&[0.0; 100], 44_100).0, [0.0; 7]);
    }
}
