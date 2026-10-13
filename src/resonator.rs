//! Synthèse modale : un objet frappé sonne par ses MODES propres.
//!
//! Un bois, une pierre ou une cloche ne se distinguent pas par la forme d'onde du coup,
//! mais par les fréquences auxquelles l'objet vibre ensuite et par la vitesse à laquelle
//! chacune s'éteint. On modélise donc l'objet par une banque de résonateurs (un par mode)
//! et on l'excite avec la couche : l'enveloppe et le bruit de la couche décrivent le COUP,
//! le résonateur décrit l'OBJET. Un coup sec de bruit blanc donne un choc franc, un bruit
//! brun plus long donne un frottement, une note tenue donne un objet qui chante.
//!
//! Les rapports de modes viennent de la physique des barres, plaques et cloches ; les
//! durées sont des valeurs de travail, à régler à l'oreille.

use std::f32::consts::TAU;

const MAX_MODES: usize = 32;
/// Durée de résonance maximale d'un mode, en secondes.
const MAX_DECAY: f32 = 8.0;
const MAX_DECAY_SCALE: f32 = 4.0;
/// Décroissance de -60 dB (ln 1000).
const T60: f32 = 6.907_755;

/// Un mode propre de l'objet.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Mode {
    /// Fréquence du mode en multiple de la hauteur de la couche (1 = fondamentale).
    pub ratio: f32,
    /// Temps pour que le mode perde 60 dB, en secondes.
    pub decay: f32,
    /// Amplitude relative du mode.
    pub gain: f32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
#[cfg_attr(feature = "schema", serde(rename_all = "snake_case"))]
pub enum Material {
    /// Barre de bois : modes très inharmoniques, aigus éteints vite. Toc sec et creux.
    Wood,
    /// Pierre, basalte : beaucoup de modes serrés, tous très courts. Choc mat et dense.
    Stone,
    /// Plaque de métal : modes serrés et longs. Résonance métallique qui tremble.
    Metal,
    /// Cloche : bourdon à l'octave inférieure, tierce mineure, longue queue.
    Bell,
    /// Verre, cristal, nacre : peu de modes, aigus et clairs, queue moyenne.
    Glass,
    /// Os, carapace creuse : modes presque harmoniques, très amortis. Toc sourd.
    Bone,
    /// Modes donnés à la main.
    Custom { modes: Vec<Mode> },
}

impl Material {
    pub fn modes(&self) -> Vec<Mode> {
        let table: &[(f32, f32, f32)] = match self {
            Material::Wood => &[
                (1.0, 0.30, 1.0),
                (2.76, 0.12, 0.55),
                (5.40, 0.06, 0.30),
                (8.93, 0.035, 0.15),
            ],
            Material::Stone => &[
                (1.0, 0.10, 1.0),
                (1.47, 0.08, 0.80),
                (2.09, 0.06, 0.70),
                (2.56, 0.05, 0.55),
                (3.39, 0.04, 0.45),
                (4.08, 0.03, 0.35),
                (5.22, 0.025, 0.25),
                (6.41, 0.02, 0.20),
            ],
            Material::Metal => &[
                (1.0, 1.6, 1.0),
                (1.59, 1.3, 0.80),
                (2.14, 1.1, 0.70),
                (2.30, 1.0, 0.60),
                (2.65, 0.9, 0.55),
                (2.92, 0.8, 0.50),
                (3.16, 0.7, 0.45),
                (3.50, 0.6, 0.40),
            ],
            Material::Bell => &[
                (0.5, 4.0, 0.6),
                (1.0, 3.0, 1.0),
                (1.19, 2.4, 0.8),
                (1.5, 2.0, 0.5),
                (2.0, 1.8, 0.9),
                (2.52, 1.2, 0.4),
                (2.66, 1.1, 0.35),
                (3.01, 0.9, 0.3),
                (4.0, 0.6, 0.2),
            ],
            Material::Glass => &[
                (1.0, 1.2, 1.0),
                (2.32, 0.9, 0.5),
                (4.25, 0.6, 0.3),
                (6.63, 0.4, 0.18),
            ],
            Material::Bone => &[
                (1.0, 0.18, 1.0),
                (2.02, 0.09, 0.4),
                (3.10, 0.05, 0.25),
                (4.30, 0.03, 0.12),
            ],
            Material::Custom { modes } => return modes.clone(),
        };
        table
            .iter()
            .map(|&(ratio, decay, gain)| Mode { ratio, decay, gain })
            .collect()
    }
}

/// L'objet qui résonne sous la couche.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Resonator {
    pub material: Material,
    /// Multiplie les durées de résonance du matériau : 0.5 = objet étouffé (bois humide),
    /// 2 = objet qui chante. 1 = naturel.
    #[cfg_attr(feature = "schema", serde(default = "one"))]
    pub decay: f32,
    /// 0 = sourd (aigus faibles et brefs), 1 = brillant. 0.5 = le matériau tel quel.
    #[cfg_attr(feature = "schema", serde(default = "half"))]
    pub brightness: f32,
    /// Part du coup qui reste audible tel quel : 0 = l'objet seul, 1 = le coup seul.
    #[cfg_attr(feature = "schema", serde(default))]
    pub dry: f32,
}

fn one() -> f32 {
    1.0
}

fn half() -> f32 {
    0.5
}

impl Resonator {
    pub fn new(material: Material) -> Self {
        Self {
            material,
            decay: one(),
            brightness: half(),
            dry: 0.0,
        }
    }

    pub fn decay(mut self, scale: f32) -> Self {
        self.decay = scale;
        self
    }

    pub fn brightness(mut self, brightness: f32) -> Self {
        self.brightness = brightness;
        self
    }

    pub fn dry(mut self, dry: f32) -> Self {
        self.dry = dry;
        self
    }

    /// Les modes après réglage de brillance et de durée. La brillance incline à la fois les
    /// gains et les durées : un objet sourd n'a pas seulement des aigus plus faibles, ils
    /// s'éteignent aussi plus vite, et c'est surtout ça que l'oreille entend.
    fn tuned_modes(&self) -> Vec<Mode> {
        let tilt = (self.brightness.clamp(0.0, 1.0) - 0.5) * 2.0;
        self.material
            .modes()
            .into_iter()
            .map(|m| Mode {
                ratio: m.ratio,
                decay: (m.decay * self.decay * m.ratio.max(0.01).powf(tilt * 0.5)).min(MAX_DECAY),
                gain: m.gain * m.ratio.max(0.01).powf(tilt),
            })
            .collect()
    }

    /// Durée de la queue ajoutée après le coup, en secondes.
    pub fn tail(&self) -> f32 {
        self.tuned_modes()
            .iter()
            .map(|m| m.decay)
            .fold(0.0, f32::max)
    }

    pub fn check(&self) -> Result<(), String> {
        let modes = self.material.modes();
        if modes.is_empty() || modes.len() > MAX_MODES {
            return Err(format!("Un résonateur a entre 1 et {MAX_MODES} modes."));
        }
        let finite = [self.decay, self.brightness, self.dry]
            .into_iter()
            .chain(modes.iter().flat_map(|m| [m.ratio, m.decay, m.gain]))
            .all(f32::is_finite);
        if !finite {
            return Err("Valeur numérique invalide dans le résonateur (NaN ou infini).".into());
        }
        if modes
            .iter()
            .any(|m| m.ratio <= 0.0 || m.decay <= 0.0 || m.decay > MAX_DECAY)
        {
            return Err(format!(
                "Chaque mode demande ratio > 0 et une durée entre 0 et {MAX_DECAY} s."
            ));
        }
        if !(self.decay > 0.0 && self.decay <= MAX_DECAY_SCALE) {
            return Err(format!(
                "decay du résonateur entre 0 et {MAX_DECAY_SCALE} (1 = naturel)."
            ));
        }
        if !(0.0..=1.0).contains(&self.brightness) || !(0.0..=1.0).contains(&self.dry) {
            return Err("brightness et dry vont de 0 à 1.".into());
        }
        Ok(())
    }

    /// Fait résonner `excitation` à la fondamentale `fundamental` (Hz). La sortie est plus
    /// longue que l'entrée de la queue de l'objet, et sa crête est ramenée à celle du coup :
    /// sinon un mode long accumulerait l'énergie du bruit et écraserait le reste du mixage.
    pub fn process(&self, excitation: &[f32], fundamental: f32, sample_rate: f32) -> Vec<f32> {
        let length = excitation.len() + (self.tail() * sample_rate).ceil() as usize;
        let mut wet = vec![0.0; length];
        let nyquist_guard = sample_rate * 0.45;

        for mode in self.tuned_modes() {
            let freq = fundamental * mode.ratio;
            if !(freq > 0.0 && freq < nyquist_guard) || mode.gain == 0.0 {
                continue;
            }
            let w = TAU * freq / sample_rate;
            let r = (-T60 / (mode.decay * sample_rate)).exp();
            let (a1, a2) = (2.0 * r * w.cos(), -r * r);
            // Avec b0 = sin(w), la réponse impulsionnelle est r^n sin((n+1)w) : une sinusoïde
            // d'amplitude 1, quelle que soit la fréquence du mode.
            let b0 = mode.gain * w.sin();
            let (mut y1, mut y2) = (0.0_f32, 0.0_f32);
            for (i, out) in wet.iter_mut().enumerate() {
                let x = excitation.get(i).copied().unwrap_or(0.0);
                let y = b0 * x + a1 * y1 + a2 * y2;
                y2 = y1;
                y1 = y;
                *out += y;
            }
        }

        let excitation_peak = peak(excitation);
        let wet_peak = peak(&wet);
        if wet_peak > f32::EPSILON {
            let scale = excitation_peak / wet_peak;
            wet.iter_mut().for_each(|s| *s *= scale);
        }
        let dry = self.dry.clamp(0.0, 1.0);
        for (i, out) in wet.iter_mut().enumerate() {
            let x = excitation.get(i).copied().unwrap_or(0.0);
            *out = *out * (1.0 - dry) + x * dry;
        }
        wet
    }
}

fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |max, s| max.max(s.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44_100.0;

    fn impulse() -> Vec<f32> {
        let mut x = vec![0.0; 32];
        x[0] = 1.0;
        x
    }

    fn single(ratio: f32, decay: f32) -> Resonator {
        Resonator::new(Material::Custom {
            modes: vec![Mode {
                ratio,
                decay,
                gain: 1.0,
            }],
        })
    }

    #[test]
    fn a_single_mode_rings_at_its_frequency() {
        let out = single(1.0, 1.0).process(&impulse(), 1000.0, SR);
        // Une seconde de sinusoïde à 1000 Hz croise zéro environ 2000 fois.
        let second = &out[..SR as usize];
        let crossings = second
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        assert!((1950..=2050).contains(&crossings), "{crossings} passages");
    }

    #[test]
    fn a_mode_loses_sixty_decibels_over_its_decay() {
        let out = single(1.0, 0.2).process(&impulse(), 500.0, SR);
        let start = peak(&out[..2000]);
        let at = (0.2 * SR) as usize;
        let late = peak(&out[at..]);
        assert!(late / start < 0.0015, "rapport {}", late / start);
    }

    #[test]
    fn output_carries_the_tail_and_stays_finite() {
        let res = Resonator::new(Material::Bell);
        let out = res.process(&impulse(), 220.0, SR);
        assert_eq!(out.len(), 32 + (res.tail() * SR).ceil() as usize);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0 + 1e-4));
    }

    #[test]
    fn brightness_shortens_and_softens_high_modes() {
        let dull = Resonator::new(Material::Metal)
            .brightness(0.0)
            .tuned_modes();
        let bright = Resonator::new(Material::Metal)
            .brightness(1.0)
            .tuned_modes();
        let last = dull.len() - 1;
        assert!(dull[last].gain < bright[last].gain);
        assert!(dull[last].decay < bright[last].decay);
        // La fondamentale (ratio 1) ne bouge pas.
        assert_eq!(dull[0], bright[0]);
    }

    #[test]
    fn check_rejects_bad_resonators() {
        assert!(Resonator::new(Material::Wood).check().is_ok());
        assert!(single(1.0, 0.0).check().is_err());
        assert!(single(1.0, 20.0).check().is_err());
        assert!(single(f32::NAN, 1.0).check().is_err());
        assert!(Resonator::new(Material::Wood).decay(0.0).check().is_err());
        assert!(Resonator::new(Material::Wood).dry(2.0).check().is_err());
        let too_many = Resonator::new(Material::Custom {
            modes: vec![
                Mode {
                    ratio: 1.0,
                    decay: 0.1,
                    gain: 1.0
                };
                40
            ],
        });
        assert!(too_many.check().is_err());
    }
}
