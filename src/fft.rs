//! FFT radix-2 et spectre de puissance fenêtré (Hann).

use std::f32::consts::PI;

/// FFT en place, itérative. `re` et `im` doivent avoir une longueur puissance de 2.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n, "taille FFT invalide");
    bit_reverse(re, im);
    let mut size = 2;
    while size <= n {
        butterflies(re, im, size);
        size *= 2;
    }
}

fn bit_reverse(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
}

fn butterflies(re: &mut [f32], im: &mut [f32], size: usize) {
    let half = size / 2;
    let angle = -2.0 * PI / size as f32;
    for start in (0..re.len()).step_by(size) {
        for k in 0..half {
            let (sin, cos) = (angle * k as f32).sin_cos();
            let (a, b) = (start + k, start + k + half);
            let tr = re[b] * cos - im[b] * sin;
            let ti = re[b] * sin + im[b] * cos;
            re[b] = re[a] - tr;
            im[b] = im[a] - ti;
            re[a] += tr;
            im[a] += ti;
        }
    }
}

pub fn hann(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos())
        .collect()
}

/// Spectre de puissance d'une trame (moitié basse, `size / 2` cases).
/// La trame est complétée par des zéros si elle est plus courte que `size`.
pub fn power_spectrum(frame: &[f32], window: &[f32]) -> Vec<f32> {
    let size = window.len();
    let mut re: Vec<f32> = (0..size)
        .map(|i| frame.get(i).copied().unwrap_or(0.0) * window[i])
        .collect();
    let mut im = vec![0.0; size];
    fft(&mut re, &mut im);
    re.iter()
        .zip(&im)
        .take(size / 2)
        .map(|(r, i)| r * r + i * i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_peaks_in_the_right_bin() {
        let size = 1024;
        let sr = 44_100.0;
        let bin_hz = sr / size as f32;
        let freq = 40.0 * bin_hz;
        let frame: Vec<f32> = (0..size)
            .map(|i| (2.0 * PI * freq * i as f32 / sr).sin())
            .collect();
        let spectrum = power_spectrum(&frame, &hann(size));
        let peak = spectrum
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i);
        assert_eq!(peak, Some(40));
    }

    #[test]
    fn impulse_has_flat_spectrum() {
        let mut re = vec![0.0; 8];
        let mut im = vec![0.0; 8];
        re[0] = 1.0;
        fft(&mut re, &mut im);
        assert!(re.iter().all(|v| (v - 1.0).abs() < 1e-6));
    }
}
