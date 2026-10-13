//! Rapports d'analyse sur des fichiers WAV : ce que Claude « entend » d'un son.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::analysis::{Stats, analyze};
use crate::bands::{BandProfile, Overlap, band_profile, overlap};
use crate::spectrogram;
use crate::wav::read_wav;

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub file: PathBuf,
    pub stats: Stats,
    pub bands: BandProfile,
}

impl Report {
    pub fn from_samples(file: &Path, samples: &[f32], sample_rate: u32) -> Self {
        Self {
            file: file.to_path_buf(),
            stats: analyze(samples, sample_rate),
            bands: band_profile(samples, sample_rate),
        }
    }

    /// Résumé sur une ligne, puis la répartition par bandes.
    pub fn detailed(&self) -> String {
        format!("{self}\n{}", self.bands)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.file.file_name().unwrap_or_default().to_string_lossy();
        write!(
            f,
            "{name:<22} {:>5.2} s  crête {:>6.1} dB  RMS {:>6.1} dB  dominante : {}",
            self.stats.duration_s,
            self.stats.peak_dbfs,
            self.stats.rms_dbfs,
            self.bands.dominant()
        )
    }
}

pub fn analyze_file(path: &Path) -> Result<Report, String> {
    let (samples, sample_rate) = read_wav(path)?;
    Ok(Report::from_samples(path, &samples, sample_rate))
}

pub fn compare_files(a: &Path, b: &Path) -> Result<(Report, Report, Overlap), String> {
    let (first, second) = (analyze_file(a)?, analyze_file(b)?);
    let result = overlap(&first.bands, &second.bands);
    Ok((first, second, result))
}

pub fn spectrogram_file(path: &Path) -> Result<Vec<u8>, String> {
    let (samples, sample_rate) = read_wav(path)?;
    Ok(spectrogram::render_png(&samples, sample_rate))
}
