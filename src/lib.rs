//! sound-forge : moteur de synthèse d'effets sonores pour jeux vidéo.
//!
//! Un [`Sound`] est fait de couches ([`Layer`]) : oscillateur, balayage de hauteur,
//! enveloppe, filtre, et éventuellement un résonateur modal qui fait de la couche le coup
//! porté à un objet (bois, pierre, cloche…). On les mixe, on applique des effets, puis on exporte en WAV.
//! Les modules d'analyse (bandes, spectrogramme) servent à vérifier un son sans l'écouter.

pub mod analysis;
pub mod bands;
pub mod base64;
pub mod effects;
pub mod envelope;
pub mod fft;
pub mod filter;
#[cfg(feature = "schema")]
pub mod instruments;
#[cfg(feature = "schema")]
pub mod music;
pub mod mutate;
pub mod oscillator;
pub mod png;
pub mod presets;
#[cfg(feature = "schema")]
pub mod recipe;
pub mod render;
pub mod report;
pub mod resonator;
pub mod rng;
pub mod sample;
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
#[cfg(feature = "schema")]
pub use recipe::Recipe;
pub use render::{SAMPLE_RATE, render};
pub use report::Report;
pub use resonator::{Material, Mode, Resonator};
pub use rng::Rng;
pub use sample::{ROOT_HZ, RECORDINGS_DIR, SampleSource};
pub use sound::{Layer, Sound};
pub use wav::{encode_wav, encode_wav_stereo, read_wav, write_wav};
pub use workspace::{ExportOptions, Variant, Workspace, render_variant, variants};
