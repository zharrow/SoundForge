//! sound-forge : moteur de synthèse d'effets sonores pour jeux vidéo.
//!
//! Un [`Sound`] est fait de couches ([`Layer`]) : oscillateur, balayage de hauteur,
//! enveloppe, filtre. On les mixe, on applique des effets, puis on exporte en WAV.
//! Les modules d'analyse (bandes, spectrogramme) servent à vérifier un son sans l'écouter.

pub mod analysis;
pub mod bands;
pub mod base64;
pub mod effects;
pub mod envelope;
pub mod fft;
pub mod filter;
pub mod mutate;
pub mod oscillator;
pub mod png;
pub mod presets;
pub mod render;
pub mod report;
pub mod rng;
pub mod sound;
pub mod spectrogram;
pub mod wav;
pub mod workspace;

pub use analysis::{Stats, analyze};
pub use bands::{BandProfile, Overlap, band_profile, overlap};
pub use envelope::{Adsr, Sweep};
pub use filter::FilterMode;
pub use mutate::vary;
pub use oscillator::Waveform;
pub use render::{SAMPLE_RATE, render};
pub use report::Report;
pub use rng::Rng;
pub use sound::{Layer, Sound};
pub use wav::{read_wav, write_wav};
pub use workspace::{ExportOptions, Workspace};
