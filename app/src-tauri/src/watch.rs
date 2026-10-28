//! Surveille le dossier de travail : ce que Claude rend par le serveur MCP, ou ce qu'un autre outil
//! écrit, apparaît dans la bibliothèque sans recharger. Un relevé toutes les 600 ms suffit : on compare
//! taille et date des WAV et des recettes, sans dépendance de surveillance du système.

use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use tauri::{AppHandle, Emitter};

const PERIOD: Duration = Duration::from_millis(600);

type Snapshot = HashMap<String, (u64, SystemTime)>;

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut last = snapshot(&crate::engine::workspace_root());
        loop {
            thread::sleep(PERIOD);
            let now = snapshot(&crate::engine::workspace_root());
            let changed = changed_sounds(&last, &now);
            if !changed.is_empty() {
                let _ = app.emit_to("main", "library-changed", changed);
            }
            last = now;
        }
    });
}

/// Les sons du dossier, et les recettes des morceaux (`musique/<nom>.json`, leurs stems suivent).
fn snapshot(root: &Path) -> Snapshot {
    let mut out = read(root, "", true);
    out.extend(read(&root.join("musique"), "musique/", false));
    out
}

fn read(dir: &Path, prefix: &str, wavs: bool) -> Snapshot {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Snapshot::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !(name.ends_with(".json") || (wavs && name.ends_with(".wav"))) {
                return None;
            }
            let meta = entry.metadata().ok()?;
            Some((format!("{prefix}{name}"), (meta.len(), meta.modified().ok()?)))
        })
        .collect()
}

/// Noms de base (sans extension ni suffixe de variante) des sons ajoutés, modifiés ou retirés.
fn changed_sounds(before: &Snapshot, after: &Snapshot) -> Vec<String> {
    let mut names: Vec<String> = after
        .iter()
        .filter(|(file, stamp)| before.get(*file) != Some(stamp))
        .map(|(file, _)| file)
        .chain(before.keys().filter(|file| !after.contains_key(*file)))
        .map(|file| base_name(file))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn base_name(file: &str) -> String {
    let stem = file.trim_end_matches(".wav").trim_end_matches(".json");
    match stem.rsplit_once('_') {
        Some((base, index)) if index.len() == 2 && index.bytes().all(|b| b.is_ascii_digit()) => base.to_owned(),
        _ => stem.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_added_changed_and_removed_sounds_once() {
        let t0 = SystemTime::UNIX_EPOCH;
        let t1 = t0 + Duration::from_secs(1);
        let before: Snapshot = [
            ("pas_01.wav".into(), (10, t0)),
            ("pas.json".into(), (5, t0)),
            ("vieux.wav".into(), (3, t0)),
        ]
        .into();
        let after: Snapshot = [
            ("pas_01.wav".into(), (10, t1)),
            ("pas.json".into(), (5, t1)),
            ("neuf_02.wav".into(), (4, t1)),
        ]
        .into();
        assert_eq!(changed_sounds(&before, &after), ["neuf", "pas", "vieux"]);
        assert!(changed_sounds(&after, &after).is_empty());
    }
}
