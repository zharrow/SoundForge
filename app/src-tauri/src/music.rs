//! Commandes de l'onglet Musique : rendre une partition en stems, les envoyer à la page, faire entendre une
//! note, enregistrer comme le serveur MCP.
//!
//! Une retouche ne change souvent qu'une piste : chaque piste rendue est gardée en mémoire, repérée par tout ce
//! qui change son son (la piste, le tempo, les mesures, les motifs, la graine, le catalogue), et seule la piste
//! modifiée est refaite. Les stems partent vers la page en binaire (WAV), sans passer par du texte.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sound_forge::instruments::{CATALOGUE_FILE, Catalogue};
use sound_forge::music::{Music, Note, Pitch, Track, Voice, assemble, catalogue_for, mixdown, render_track};
use sound_forge::{SAMPLE_RATE, encode_wav_stereo, spectrogram, workspace::MusicRecipe};
use tauri::ipc::Response;
use tauri::State;

use crate::engine::workspace;

const CACHE_LIMIT: usize = 64;

#[derive(Default)]
pub struct MusicState {
    /// Pistes rendues (mono, boucle + queue), par empreinte.
    tracks: Mutex<HashMap<u64, Arc<Vec<f32>>>>,
    /// Derniers stems rendus, en WAV, pour `music_stem`.
    stems: Mutex<HashMap<String, Vec<u8>>>,
}

#[derive(Debug, Serialize)]
pub struct Piece {
    name: String,
    bpm: f32,
    bars: u32,
    tracks: usize,
    modified: u64,
}

#[derive(Debug, Serialize)]
pub struct InstrumentInfo {
    id: String,
    name: String,
    family: String,
    kind: String,
    range: (u8, u8),
    layers: u8,
    /// Notes d'un kit de percussions (vide pour un instrument à hauteurs).
    kit: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct MusicLibrary {
    pieces: Vec<Piece>,
    instruments: Vec<InstrumentInfo>,
    /// Erreur de catalogue, s'il manque : l'onglet reste utilisable avec des sons de synthèse.
    catalogue_error: Option<String>,
}

#[tauri::command]
pub fn music_library() -> MusicLibrary {
    let ws = workspace();
    let dir = ws.root().join(sound_forge::workspace::MUSIC_DIR);
    let mut pieces: Vec<Piece> = ws
        .list_music()
        .into_iter()
        .filter_map(|name| {
            let recipe = ws.read_music(&name).ok()?;
            let modified = std::fs::metadata(dir.join(format!("{name}.json")))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            Some(Piece { bpm: recipe.music.bpm, bars: recipe.music.bars, tracks: recipe.music.tracks.len(), modified, name })
        })
        .collect();
    pieces.sort_by(|a, b| a.name.cmp(&b.name));
    let (instruments, catalogue_error) = match ws.instruments() {
        Ok(catalogue) => (catalogue.instruments.iter().map(info).collect(), None),
        Err(error) => (Vec::new(), Some(error)),
    };
    MusicLibrary { pieces, instruments, catalogue_error }
}

fn info(i: &sound_forge::instruments::Instrument) -> InstrumentInfo {
    let mut notes: Vec<u8> = i.zones.iter().map(|z| z.note).collect();
    notes.sort_unstable();
    notes.dedup();
    InstrumentInfo {
        id: i.id.clone(),
        name: i.name.clone(),
        family: i.family.clone(),
        kind: format!("{:?}", i.kind).to_lowercase(),
        range: i.range,
        layers: i.layers,
        kit: if i.family == "percussions" { notes } else { Vec::new() },
    }
}

#[tauri::command]
pub fn load_music(name: String) -> Result<MusicRecipe, String> {
    workspace().read_music(&name)
}

#[derive(Debug, Serialize)]
pub struct RenderedMusic {
    /// Stems, dans l'ordre d'apparition : nom et niveau efficace (dBFS).
    stems: Vec<(String, f32)>,
    duration: f32,
    frames: usize,
    /// Pistes réellement refaites (les autres venaient de la mémoire).
    rendered: usize,
    /// Spectrogramme du mélange (PNG, base64).
    spectrum: String,
}

/// Rend la partition ; les stems sont ensuite lus un par un avec `music_stem`.
#[tauri::command]
pub async fn render_music_take(music: Music, seed: u64, state: State<'_, MusicState>) -> Result<RenderedMusic, String> {
    music.check()?;
    let instruments = workspace().instruments_dir();
    let catalogue_stamp = std::fs::metadata(instruments.join(CATALOGUE_FILE))
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let context = serde_json::to_string(&(music.bpm, music.beats_per_bar, music.bars, &music.patterns, seed, catalogue_stamp))
        .map_err(|e| e.to_string())?;
    let keys: Vec<u64> = music
        .tracks
        .iter()
        .map(|track| {
            let mut hasher = DefaultHasher::new();
            context.hash(&mut hasher);
            serde_json::to_string(track).unwrap_or_default().hash(&mut hasher);
            hasher.finish()
        })
        .collect();
    let cached: Vec<Option<Arc<Vec<f32>>>> = {
        let cache = state.tracks.lock().unwrap();
        keys.iter().map(|k| cache.get(k).cloned()).collect()
    };
    let missing = cached.iter().filter(|c| c.is_none()).count();
    let music_for_render = music.clone();
    let (tracks, stems_wav, stems_info, spectrum) = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        let catalogue: Catalogue = if missing > 0 { catalogue_for(&music_for_render, &instruments)? } else { Catalogue::default() };
        let tracks: Vec<Arc<Vec<f32>>> = music_for_render
            .tracks
            .iter()
            .zip(cached)
            .map(|(track, cached)| match cached {
                Some(mono) => Ok(mono),
                None => render_track(&music_for_render, track, &catalogue, &instruments, seed).map(Arc::new),
            })
            .collect::<Result<_, String>>()?;
        let plain: Vec<Vec<f32>> = tracks.iter().map(|t| t.as_ref().clone()).collect();
        let stems = assemble(&music_for_render, &plain);
        let mix = mixdown(&stems);
        let mono: Vec<f32> = mix.left.iter().zip(&mix.right).map(|(l, r)| (l + r) * 0.5).collect();
        let spectrum = sound_forge::base64::encode(&spectrogram::render_spectrum_png(&mono, SAMPLE_RATE));
        let info: Vec<(String, f32)> = stems
            .iter()
            .map(|s| {
                let energy: f32 = s.left.iter().chain(&s.right).map(|v| v * v).sum();
                let rms = (energy / (2 * s.left.len().max(1)) as f32).sqrt();
                (s.name.clone(), 20.0 * rms.max(1e-9).log10())
            })
            .collect();
        let wavs: HashMap<String, Vec<u8>> = stems
            .iter()
            .map(|s| (s.name.clone(), encode_wav_stereo(&s.left, &s.right, SAMPLE_RATE)))
            .collect();
        Ok((tracks, wavs, info, spectrum))
    })
    .await
    .map_err(|e| e.to_string())??;
    {
        let mut cache = state.tracks.lock().unwrap();
        if cache.len() > CACHE_LIMIT {
            cache.clear();
        }
        for (key, track) in keys.into_iter().zip(tracks) {
            cache.insert(key, track);
        }
    }
    *state.stems.lock().unwrap() = stems_wav;
    Ok(RenderedMusic {
        stems: stems_info,
        duration: music.duration(),
        frames: music.length(),
        rendered: missing,
        spectrum,
    })
}

/// Un stem du dernier rendu, en WAV binaire.
#[tauri::command]
pub fn music_stem(name: String, state: State<'_, MusicState>) -> Result<Response, String> {
    state
        .stems
        .lock()
        .unwrap()
        .get(&name)
        .cloned()
        .map(Response::new)
        .ok_or_else(|| format!("Stem inconnu : {name}"))
}

/// Une note seule, pour l'entendre en la posant (WAV binaire).
#[tauri::command]
pub async fn preview_note(instrument: Voice, note: u8, vel: f32, seconds: f32) -> Result<Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let beats = (seconds * 2.0).clamp(0.25, 8.0);
        let music = Music {
            name: "note".into(),
            bpm: 120.0,
            beats_per_bar: 4,
            bars: 1,
            looping: false,
            patterns: Default::default(),
            tracks: vec![Track {
                name: "note".into(),
                stem: None,
                instrument,
                volume_db: 0.0,
                pan: 0.0,
                reverb: 0.0,
                transpose: 0,
                humanize: 0.0,
                notes: vec![Note { at: 0.0, len: beats, note: Pitch::Midi(note), vel }],
                play: Vec::new(),
            }],
            reverb: None,
            peak_db: -6.0,
        };
        let stems = sound_forge::music::render_music(&music, &workspace().instruments_dir(), 1)?;
        let mix = mixdown(&stems);
        Ok(Response::new(encode_wav_stereo(&mix.left, &mix.right, SAMPLE_RATE)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Enregistre le morceau (stems, mélange, recette), exactement comme render_music du serveur MCP.
#[tauri::command]
pub async fn save_music(name: String, music: Music, seed: u64) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let reports = workspace().render_music(&music, &name, seed)?;
        Ok(reports.iter().map(|r| r.file_name()).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn music_exists(name: String) -> bool {
    workspace().read_music(&name).is_ok()
}
