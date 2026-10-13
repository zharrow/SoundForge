//! Effets appliqués au mixage final : saturation, écho, réverbération.

/// Écho : `time` en secondes, `feedback` (0..0.9) règle le nombre de répétitions.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Echo {
    /// Délai entre deux répétitions, en secondes.
    pub time: f32,
    /// Part réinjectée à chaque répétition, de 0 à 0.9.
    pub feedback: f32,
    /// Volume des répétitions, de 0 à 1.
    pub mix: f32,
}

/// Réverbération : `room` (0..1) règle la taille de la pièce, `damping` (0..1) étouffe les aigus.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct Reverb {
    /// Taille de la pièce, de 0 (placard) à 1 (cathédrale).
    pub room: f32,
    /// Étouffement des aigus de la queue, de 0 (brillant) à 1 (sourd).
    pub damping: f32,
    /// Volume de la réverbération, de 0 à 1 (0.1 à 0.3 en général).
    pub mix: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(
    feature = "schema",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
#[cfg_attr(feature = "schema", serde(default))]
pub struct Effects {
    /// Saturation douce : 1 = légère, 4 = très agressive.
    pub drive: Option<f32>,
    pub echo: Option<Echo>,
    pub reverb: Option<Reverb>,
}

const MAX_TAIL: f32 = 4.0;

impl Effects {
    /// Durée ajoutée en fin de son pour laisser s'éteindre écho et réverbération.
    pub fn tail(&self) -> f32 {
        let echo = self.echo.map_or(0.0, |e| e.tail());
        let reverb = self
            .reverb
            .map_or(0.0, |r| 0.4 + 3.0 * r.room.clamp(0.0, 1.0));
        echo.max(reverb).min(MAX_TAIL)
    }

    pub fn apply(&self, buffer: &mut [f32], sample_rate: f32) {
        if let Some(drive) = self.drive {
            saturate(buffer, drive);
        }
        if let Some(echo) = self.echo {
            apply_echo(buffer, echo, sample_rate);
        }
        if let Some(reverb) = self.reverb {
            apply_reverb(buffer, reverb, sample_rate);
        }
    }
}

impl Echo {
    fn tail(&self) -> f32 {
        let feedback = self.feedback.clamp(0.01, 0.9);
        let repeats = (0.001_f32.ln() / feedback.ln()).ceil();
        self.time * repeats
    }
}

fn saturate(buffer: &mut [f32], drive: f32) {
    let drive = drive.max(0.1);
    let norm = drive.tanh();
    for sample in buffer.iter_mut() {
        *sample = (*sample * drive).tanh() / norm;
    }
}

fn apply_echo(buffer: &mut [f32], echo: Echo, sample_rate: f32) {
    let delay = ((echo.time * sample_rate) as usize).max(1);
    let feedback = echo.feedback.clamp(0.0, 0.9);
    let mut line = vec![0.0_f32; delay];
    for (i, sample) in buffer.iter_mut().enumerate() {
        let index = i % delay;
        let delayed = line[index];
        line[index] = *sample + delayed * feedback;
        *sample += delayed * echo.mix;
    }
}

// ---------- Réverbération (algorithme Freeverb, version mono) ----------

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const INPUT_GAIN: f32 = 0.015;
const WET_GAIN: f32 = 3.0;

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    store: f32,
}

impl Comb {
    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = output * (1.0 - damping) + self.store * damping;
        self.buffer[self.index] = input + self.store * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct AllPass {
    buffer: Vec<f32>,
    index: usize,
}

impl AllPass {
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        self.buffer[self.index] = input + buffered * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        buffered - input
    }
}

fn scaled_length(tuning: usize, sample_rate: f32) -> usize {
    ((tuning as f32 * sample_rate / 44_100.0) as usize).max(1)
}

fn apply_reverb(buffer: &mut [f32], reverb: Reverb, sample_rate: f32) {
    let mut combs: Vec<Comb> = COMB_TUNING
        .iter()
        .map(|&t| Comb {
            buffer: vec![0.0; scaled_length(t, sample_rate)],
            index: 0,
            store: 0.0,
        })
        .collect();
    let mut allpasses: Vec<AllPass> = ALLPASS_TUNING
        .iter()
        .map(|&t| AllPass {
            buffer: vec![0.0; scaled_length(t, sample_rate)],
            index: 0,
        })
        .collect();
    let feedback = 0.7 + 0.28 * reverb.room.clamp(0.0, 1.0);
    let damping = reverb.damping.clamp(0.0, 1.0);

    for sample in buffer.iter_mut() {
        let input = *sample * INPUT_GAIN;
        let mut wet: f32 = combs
            .iter_mut()
            .map(|c| c.process(input, feedback, damping))
            .sum();
        wet = allpasses.iter_mut().fold(wet, |acc, a| a.process(acc));
        *sample += wet * WET_GAIN * reverb.mix;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(length: usize) -> Vec<f32> {
        let mut buffer = vec![0.0; length];
        buffer[0] = 1.0;
        buffer
    }

    #[test]
    fn echo_repeats_the_impulse() {
        let mut buffer = impulse(1000);
        apply_echo(
            &mut buffer,
            Echo {
                time: 0.01,
                feedback: 0.5,
                mix: 0.5,
            },
            10_000.0,
        );
        assert_eq!(buffer[100], 0.5);
        assert_eq!(buffer[200], 0.25);
    }

    #[test]
    fn reverb_adds_a_tail() {
        let mut buffer = impulse(44_100);
        apply_reverb(
            &mut buffer,
            Reverb {
                room: 0.5,
                damping: 0.3,
                mix: 1.0,
            },
            44_100.0,
        );
        let tail_energy: f32 = buffer[4410..].iter().map(|s| s * s).sum();
        assert!(tail_energy > 0.0);
    }

    #[test]
    fn saturation_keeps_full_scale() {
        let mut buffer = vec![1.0, -1.0, 0.0];
        saturate(&mut buffer, 3.0);
        assert!((buffer[0] - 1.0).abs() < 1e-6 && (buffer[1] + 1.0).abs() < 1e-6);
    }
}
