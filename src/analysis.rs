//! Mesures d'un son rendu. Elles servent de repère à Claude, qui ne peut pas écouter.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub duration_s: f32,
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
}

pub fn analyze(samples: &[f32], sample_rate: u32) -> Stats {
    let peak = samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32;
    Stats {
        duration_s: samples.len() as f32 / sample_rate as f32,
        peak_dbfs: to_db(peak),
        rms_dbfs: to_db(mean_square.sqrt()),
    }
}

fn to_db(amplitude: f32) -> f32 {
    20.0 * amplitude.max(1e-9).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scale_square_is_zero_db() {
        let stats = analyze(&[1.0, -1.0, 1.0, -1.0], 4);
        assert!(stats.peak_dbfs.abs() < 1e-4 && stats.rms_dbfs.abs() < 1e-4);
        assert_eq!(stats.duration_s, 1.0);
    }
}
