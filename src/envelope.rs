//! Enveloppes : volume dans le temps (ADSR) et balayages de fréquence (Sweep).

/// Enveloppe de volume. `hold` remplace la durée d'appui d'une note :
/// un effet sonore n'a pas de touche relâchée, sa durée est fixe.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Adsr {
    /// Montée du silence au volume maximal, en secondes (0.001 = attaque sèche).
    pub attack: f32,
    /// Descente du maximum vers `sustain`, en secondes.
    pub decay: f32,
    /// Niveau de palier, de 0 à 1. 0 = son percussif qui s'éteint après `decay`.
    #[cfg_attr(feature = "schema", serde(default))]
    pub sustain: f32,
    /// Durée du palier, en secondes.
    #[cfg_attr(feature = "schema", serde(default))]
    pub hold: f32,
    /// Extinction finale depuis `sustain`, en secondes.
    #[cfg_attr(feature = "schema", serde(default))]
    pub release: f32,
}

impl Adsr {
    pub fn new(attack: f32, decay: f32, sustain: f32, hold: f32, release: f32) -> Self {
        Self {
            attack,
            decay,
            sustain: sustain.clamp(0.0, 1.0),
            hold,
            release,
        }
    }

    /// Son percussif : montée puis extinction, sans palier.
    pub fn percussive(attack: f32, decay: f32) -> Self {
        Self::new(attack, decay, 0.0, 0.0, 0.0)
    }

    pub fn duration(&self) -> f32 {
        self.attack + self.decay + self.hold + self.release
    }

    /// Volume entre 0 et 1 à l'instant `t` (secondes).
    pub fn value(&self, t: f32) -> f32 {
        let mut t = t;
        if t < self.attack {
            return t / self.attack;
        }
        t -= self.attack;
        if t < self.decay {
            let remaining = 1.0 - t / self.decay;
            return self.sustain + (1.0 - self.sustain) * remaining * remaining;
        }
        t -= self.decay;
        if t < self.hold {
            return self.sustain;
        }
        t -= self.hold;
        if t < self.release {
            let remaining = 1.0 - t / self.release;
            return self.sustain * remaining * remaining;
        }
        0.0
    }

    /// Allonge ou raccourcit toutes les phases (utilisé pour les variantes).
    pub fn stretched(&self, ratio: f32) -> Self {
        Self {
            attack: self.attack * ratio,
            decay: self.decay * ratio,
            hold: self.hold * ratio,
            release: self.release * ratio,
            ..*self
        }
    }
}

/// Glissement exponentiel d'une valeur (fréquence en Hz) de `start` à `end` en `time` secondes.
/// L'exponentiel correspond à la perception de l'oreille : chaque octave dure autant.
/// Pour une valeur fixe : `end` = `start` et `time` = 0.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Sweep {
    /// Valeur de départ (Hz pour une hauteur ou une coupure de filtre).
    pub start: f32,
    /// Valeur d'arrivée.
    pub end: f32,
    /// Durée du glissement en secondes ; ensuite la valeur reste à `end`.
    pub time: f32,
}

const MIN_VALUE: f32 = 1e-3;

impl Sweep {
    pub fn new(start: f32, end: f32, time: f32) -> Self {
        Self {
            start: start.max(MIN_VALUE),
            end: end.max(MIN_VALUE),
            time,
        }
    }

    pub fn constant(value: f32) -> Self {
        Self::new(value, value, 0.0)
    }

    pub fn value(&self, t: f32) -> f32 {
        let progress = if self.time > 0.0 {
            (t / self.time).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let (start, end) = (self.start.max(MIN_VALUE), self.end.max(MIN_VALUE));
        start * (end / start).powf(progress)
    }

    pub fn scaled(&self, ratio: f32) -> Self {
        Self::new(self.start * ratio, self.end * ratio, self.time)
    }

    pub fn stretched(&self, ratio: f32) -> Self {
        Self {
            time: self.time * ratio,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adsr_shape() {
        let env = Adsr::new(0.1, 0.1, 0.5, 0.2, 0.1);
        assert!((env.duration() - 0.5).abs() < 1e-6);
        assert_eq!(env.value(0.0), 0.0);
        assert!((env.value(0.05) - 0.5).abs() < 1e-6);
        assert!((env.value(0.3) - 0.5).abs() < 1e-6);
        assert_eq!(env.value(0.6), 0.0);
    }

    #[test]
    fn percussive_without_attack_starts_at_full_volume() {
        assert_eq!(Adsr::percussive(0.0, 0.2).value(0.0), 1.0);
    }

    #[test]
    fn sweep_passes_through_geometric_middle() {
        let sweep = Sweep::new(100.0, 400.0, 1.0);
        assert!((sweep.value(0.5) - 200.0).abs() < 0.01);
        assert_eq!(sweep.value(5.0), 400.0);
    }
}
