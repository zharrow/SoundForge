//! Rendu d'un `Sound` en échantillons, puis mastering (nettoyage et normalisation).

use std::f32::consts::TAU;

use crate::filter::Svf;
use crate::oscillator::Oscillator;
use crate::rng::Rng;
use crate::sound::{FilterSpec, Layer, Sound, Vibrato};

/// Fréquence recommandée par Godot pour les effets sonores.
pub const SAMPLE_RATE: u32 = 44_100;

const SILENCE_THRESHOLD: f32 = 0.001; // -60 dB sous la crête
const FADE_OUT_SECONDS: f32 = 0.005;

/// Rend le son en mono, normalisé à `sound.peak_db`. Même graine = même résultat.
pub fn render(sound: &Sound, seed: u64) -> Vec<f32> {
    let sample_rate = SAMPLE_RATE as f32;
    let mut rng = Rng::new(seed);
    let length = ((sound.duration() + sound.effects.tail()) * sample_rate).ceil() as usize;
    let mut buffer = vec![0.0; length.max(1)];

    for layer in &sound.layers {
        mix_layer(&mut buffer, layer, sample_rate, &mut rng);
    }
    sound.effects.apply(&mut buffer, sample_rate);
    master(&mut buffer, sound.peak_db, sample_rate);
    buffer
}

fn mix_layer(buffer: &mut [f32], layer: &Layer, sample_rate: f32, rng: &mut Rng) {
    let offset = ((layer.start * sample_rate) as usize).min(buffer.len());
    let samples = render_layer(layer, sample_rate, rng);
    for (out, sample) in buffer[offset..].iter_mut().zip(samples) {
        *out += sample;
    }
}

struct Voice {
    oscillator: Oscillator,
    ratio: f32,
}

fn render_layer(layer: &Layer, sample_rate: f32, rng: &mut Rng) -> Vec<f32> {
    let strike = render_strike(layer, sample_rate, rng);
    match &layer.resonator {
        Some(resonator) => resonator.process(&strike, layer.pitch.start, sample_rate),
        None => strike,
    }
}

/// La couche elle-même : oscillateurs, filtre, enveloppe. Sous un résonateur, c'est le coup.
fn render_strike(layer: &Layer, sample_rate: f32, rng: &mut Rng) -> Vec<f32> {
    let count = (layer.envelope.duration() * sample_rate).ceil() as usize;
    let mut voices = make_voices(layer, rng);
    let normalization = 1.0 / (voices.len() as f32).sqrt();
    let mut filter = Svf::default();

    (0..count)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let freq = layer.pitch.value(t) * vibrato_ratio(layer.vibrato, t);
            let raw: f32 = voices
                .iter_mut()
                .map(|v| v.oscillator.next(freq * v.ratio, sample_rate, rng))
                .sum();
            let filtered = apply_filter(
                &mut filter,
                layer.filter,
                raw * normalization,
                t,
                sample_rate,
            );
            filtered * layer.envelope.value(t) * layer.gain
        })
        .collect()
}

/// Répartit les voix d'unisson symétriquement autour de la note, phases aléatoires.
fn make_voices(layer: &Layer, rng: &mut Rng) -> Vec<Voice> {
    let count = layer.unison.voices.max(1);
    (0..count)
        .map(|i| {
            let spread = if count == 1 {
                0.0
            } else {
                i as f32 / (count - 1) as f32 * 2.0 - 1.0
            };
            let cents = spread * layer.unison.detune_cents;
            Voice {
                oscillator: Oscillator::new(layer.waveform, rng.unit()),
                ratio: 2_f32.powf(cents / 1200.0),
            }
        })
        .collect()
}

fn vibrato_ratio(vibrato: Option<Vibrato>, t: f32) -> f32 {
    vibrato.map_or(1.0, |v| {
        2_f32.powf(v.depth_semitones * (TAU * v.rate_hz * t).sin() / 12.0)
    })
}

fn apply_filter(
    filter: &mut Svf,
    spec: Option<FilterSpec>,
    input: f32,
    t: f32,
    sample_rate: f32,
) -> f32 {
    match spec {
        Some(spec) => filter.process(
            input,
            spec.cutoff.value(t),
            spec.resonance,
            spec.mode,
            sample_rate,
        ),
        None => input,
    }
}

// ---------- Mastering ----------

fn master(buffer: &mut Vec<f32>, peak_db: f32, sample_rate: f32) {
    remove_dc(buffer);
    trim_silence(buffer, sample_rate);
    fade_out(buffer, sample_rate);
    normalize(buffer, peak_db);
}

/// Filtre passe-haut très bas : supprime le décalage continu qui provoque des clics.
fn remove_dc(buffer: &mut [f32]) {
    let (mut previous_in, mut previous_out) = (0.0, 0.0);
    for sample in buffer.iter_mut() {
        let output = *sample - previous_in + 0.995 * previous_out;
        previous_in = *sample;
        previous_out = output;
        *sample = output;
    }
}

fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |max, s| max.max(s.abs()))
}

/// Coupe le silence en fin de son, pour ne pas garder des secondes inutiles.
fn trim_silence(buffer: &mut Vec<f32>, sample_rate: f32) {
    let threshold = peak(buffer) * SILENCE_THRESHOLD;
    let last_audible = buffer
        .iter()
        .rposition(|s| s.abs() > threshold)
        .unwrap_or(0);
    let fade = (FADE_OUT_SECONDS * sample_rate) as usize;
    buffer.truncate((last_audible + fade).min(buffer.len()).max(1));
}

fn fade_out(buffer: &mut [f32], sample_rate: f32) {
    let fade = ((FADE_OUT_SECONDS * sample_rate) as usize).min(buffer.len());
    let start = buffer.len() - fade;
    for (i, sample) in buffer[start..].iter_mut().enumerate() {
        *sample *= 1.0 - i as f32 / fade as f32;
    }
}

fn normalize(buffer: &mut [f32], peak_db: f32) {
    let current = peak(buffer);
    if current <= f32::EPSILON {
        return;
    }
    let gain = 10_f32.powf(peak_db / 20.0) / current;
    buffer.iter_mut().for_each(|s| *s *= gain);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Adsr, Sweep};
    use crate::oscillator::Waveform;

    fn beep() -> Sound {
        Sound::new("beep")
            .layer(Layer::new(
                Waveform::Sine,
                Sweep::constant(440.0),
                Adsr::percussive(0.01, 0.2),
            ))
            .peak_db(-3.0)
    }

    #[test]
    fn render_reaches_target_peak() {
        let samples = render(&beep(), 1);
        let peak_db = 20.0 * peak(&samples).log10();
        assert!((peak_db + 3.0).abs() < 0.01);
    }

    #[test]
    fn render_is_deterministic() {
        let noisy = beep().layer(Layer::new(
            Waveform::WhiteNoise,
            Sweep::constant(1.0),
            Adsr::percussive(0.0, 0.1),
        ));
        assert_eq!(render(&noisy, 9), render(&noisy, 9));
        assert_ne!(render(&noisy, 9), render(&noisy, 10));
    }

    #[test]
    fn render_ends_silently() {
        let samples = render(&beep().reverb(0.5, 0.3, 0.3), 1);
        assert!(samples.last().unwrap().abs() < 1e-3);
    }
}
