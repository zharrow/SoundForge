//! Sound Forge : coquille native de l'éditeur de sons.
//!
//! Le son en cours d'édition vit dans la page (TypeScript). Ce côté Rust embarque le moteur
//! sound-forge (rendu, variantes, analyse, recettes), fournit la fenêtre et les menus, surveille le
//! dossier de travail et ouvre le pont local par lequel le serveur MCP de Claude pilote l'éditeur.

mod bridge;
mod engine;
mod menu;
mod music;
mod watch;

use std::path::PathBuf;

use tauri::RunEvent;

pub(crate) fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(bridge::Pending::default())
        .manage(music::MusicState::default())
        .invoke_handler(tauri::generate_handler![
            engine::render,
            engine::library,
            engine::load_recipe,
            engine::preset,
            engine::save,
            engine::exists,
            engine::reveal,
            engine::save_recording,
            engine::recording_wav,
            music::music_library,
            music::load_music,
            music::render_music_take,
            music::music_stem,
            music::preview_note,
            music::save_music,
            music::music_exists,
            bridge::bridge_ready,
            bridge::rpc_reply,
        ])
        .setup(|app| {
            menu::install(app)?;
            bridge::start(app.handle().clone())?;
            watch::start(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("impossible de démarrer Sound Forge");

    app.run(|_handle, event| {
        if let RunEvent::Exit = event {
            bridge::cleanup();
        }
    });
}
