//! Image de contrôle d'un son : forme d'onde en haut, spectrogramme en dessous.
//!
//! Lecture : le temps va de gauche à droite. L'axe vertical du spectrogramme est
//! logarithmique, de 40 Hz (bas) à 20 kHz (haut). Lignes repères à 100 Hz, 1 kHz et 10 kHz.
//! Couleur : noir = silence, violet → orange → jaune pâle = de plus en plus fort.

use crate::fft::{hann, power_spectrum};
use crate::png::RgbImage;

pub const WIDTH: usize = 480;
const WAVE_HEIGHT: usize = 64;
const SPECTRUM_HEIGHT: usize = 240;
const GAP: usize = 4;
const FFT_SIZE: usize = 2048;
const MIN_FREQ: f32 = 40.0;
const MAX_FREQ: f32 = 20_000.0;
const DB_RANGE: f32 = 90.0;
const GRID_FREQS: [f32; 3] = [100.0, 1000.0, 10_000.0];

const BACKGROUND: [u8; 3] = [18, 18, 24];
const WAVE_COLOR: [u8; 3] = [110, 190, 255];
const GRID_COLOR: [u8; 3] = [90, 90, 100];
const COLORMAP: [[f32; 3]; 5] = [
    [0.0, 0.0, 4.0],
    [60.0, 15.0, 110.0],
    [180.0, 55.0, 120.0],
    [250.0, 140.0, 90.0],
    [252.0, 250.0, 190.0],
];

pub fn render_png(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let height = WAVE_HEIGHT + GAP + SPECTRUM_HEIGHT;
    let mut image = RgbImage::new(WIDTH, height, BACKGROUND);
    draw_waveform(&mut image, samples);
    draw_spectrogram(&mut image, samples, sample_rate, WAVE_HEIGHT + GAP);
    image.to_png()
}

/// Le spectrogramme seul (480 × 240), pour un éditeur qui dessine lui-même la forme d'onde.
pub fn render_spectrum_png(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let mut image = RgbImage::new(WIDTH, SPECTRUM_HEIGHT, BACKGROUND);
    draw_spectrogram(&mut image, samples, sample_rate, 0);
    image.to_png()
}

fn column_range(samples: &[f32], column: usize) -> &[f32] {
    let start = column * samples.len() / WIDTH;
    let end = ((column + 1) * samples.len() / WIDTH)
        .max(start + 1)
        .min(samples.len());
    &samples[start.min(end)..end]
}

fn draw_waveform(image: &mut RgbImage, samples: &[f32]) {
    if samples.is_empty() {
        return;
    }
    let center = WAVE_HEIGHT as f32 / 2.0;
    for x in 0..WIDTH {
        let segment = column_range(samples, x);
        let (low, high) = segment
            .iter()
            .fold((0.0_f32, 0.0_f32), |(lo, hi), &s| (lo.min(s), hi.max(s)));
        let top = (center - high.clamp(-1.0, 1.0) * center) as usize;
        let bottom = (center - low.clamp(-1.0, 1.0) * center) as usize;
        for y in top..=bottom.min(WAVE_HEIGHT - 1) {
            image.set(x, y, WAVE_COLOR);
        }
    }
}

fn draw_spectrogram(image: &mut RgbImage, samples: &[f32], sample_rate: u32, top: usize) {
    let columns = spectrum_columns(samples);
    let peak = columns
        .iter()
        .flatten()
        .fold(f32::EPSILON, |m, &p| m.max(p));
    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    for (x, spectrum) in columns.iter().enumerate() {
        for row in 0..SPECTRUM_HEIGHT {
            let bin = ((row_frequency(row) / bin_hz) as usize).min(spectrum.len() - 1);
            let db = 10.0 * (spectrum[bin] / peak).max(1e-12).log10();
            image.set(x, top + row, colormap(1.0 + db / DB_RANGE));
        }
    }
    draw_grid(image, top);
}

/// Un spectre par colonne de pixels, centré sur la portion de son correspondante.
fn spectrum_columns(samples: &[f32]) -> Vec<Vec<f32>> {
    let window = hann(FFT_SIZE);
    (0..WIDTH)
        .map(|x| {
            let center = x * samples.len() / WIDTH;
            let start = center.saturating_sub(FFT_SIZE / 2);
            let end = (start + FFT_SIZE).min(samples.len());
            power_spectrum(&samples[start..end], &window)
        })
        .collect()
}

/// Fréquence affichée à une rangée : rangée 0 = haut = MAX_FREQ.
fn row_frequency(row: usize) -> f32 {
    let position = 1.0 - row as f32 / (SPECTRUM_HEIGHT - 1) as f32;
    MIN_FREQ * (MAX_FREQ / MIN_FREQ).powf(position)
}

fn frequency_row(freq: f32) -> usize {
    let position = (freq / MIN_FREQ).ln() / (MAX_FREQ / MIN_FREQ).ln();
    ((1.0 - position) * (SPECTRUM_HEIGHT - 1) as f32).round() as usize
}

fn draw_grid(image: &mut RgbImage, top: usize) {
    for freq in GRID_FREQS {
        let y = top + frequency_row(freq);
        for x in (0..WIDTH).step_by(4) {
            image.set(x, y, GRID_COLOR);
            image.set(x + 1, y, GRID_COLOR);
        }
    }
}

fn colormap(level: f32) -> [u8; 3] {
    let scaled = level.clamp(0.0, 1.0) * (COLORMAP.len() - 1) as f32;
    let index = (scaled as usize).min(COLORMAP.len() - 2);
    let t = scaled - index as f32;
    let (a, b) = (COLORMAP[index], COLORMAP[index + 1]);
    [0, 1, 2].map(|c| (a[c] + (b[c] - a[c]) * t) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_axis_round_trips() {
        assert_eq!(frequency_row(MAX_FREQ), 0);
        assert_eq!(frequency_row(MIN_FREQ), SPECTRUM_HEIGHT - 1);
        assert!((row_frequency(frequency_row(1000.0)) - 1000.0).abs() < 30.0);
    }

    #[test]
    fn colormap_extremes() {
        assert_eq!(colormap(0.0), [0, 0, 4]);
        assert_eq!(colormap(1.0), [252, 250, 190]);
    }

    #[test]
    fn renders_png_for_short_and_empty_sounds() {
        assert_eq!(&render_png(&[0.1; 300], 44_100)[1..4], b"PNG");
        assert_eq!(&render_png(&[], 44_100)[1..4], b"PNG");
    }
}
