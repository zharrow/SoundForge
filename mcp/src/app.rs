//! Client du pont local de l'app Sound Forge (voir app/src-tauri/src/bridge.rs).
//!
//! L'app écrit son port et un jeton secret dans ~/.sound-forge/app.json. Si elle n'est pas lancée,
//! `call` l'ouvre et attend qu'elle soit prête ; `call_if_running` ne la lance jamais (pour montrer
//! dans l'app un son que Claude vient de rendre, seulement si elle est déjà ouverte).
//! HTTP minimal sur TcpStream : une requête, une réponse, connexion fermée, sans dépendance.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BUNDLE_ID: &str = "local.sound-forge.editor";
const APP_NAME: &str = "Sound Forge.app";
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(25);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const QUICK_TIMEOUT: Duration = Duration::from_secs(5);

struct Info {
    port: u16,
    token: String,
}

/// Appelle une méthode de l'éditeur, en le lançant si besoin. Une erreur de l'éditeur revient en texte.
pub fn call(method: &str, params: Value) -> Result<Value, String> {
    let info = connect()?;
    post(&info, method, params, CALL_TIMEOUT)
}

/// Comme `call`, mais seulement si l'app tourne déjà ; `None` sinon.
pub fn call_if_running(method: &str, params: Value) -> Option<Result<Value, String>> {
    let info = read_info().filter(alive)?;
    Some(post(&info, method, params, QUICK_TIMEOUT))
}

/// Met l'app au premier plan (et la lance si besoin).
pub fn bring_to_front() -> Result<(), String> {
    connect()?;
    launch().map(|_| ())
}

fn connect() -> Result<Info, String> {
    if let Some(info) = read_info().filter(alive) {
        return Ok(info);
    }
    launch()?;
    let deadline = Instant::now() + LAUNCH_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(info) = read_info().filter(alive) {
            return Ok(info);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "Impossible de joindre l'app Sound Forge. Vérifier qu'elle est installée \
         (cd app && npm run install-app, ou chemin dans la variable SOUND_FORGE_APP) : {}",
        repo_build().display()
    ))
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn repo_build() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../app/src-tauri/target/release/bundle/macos")
        .join(APP_NAME)
}

/// Ordre de recherche : SOUND_FORGE_APP, l'identifiant de l'app (LaunchServices), ~/Applications, /Applications,
/// puis le build du dépôt.
fn launch() -> Result<(), String> {
    if let Some(path) = std::env::var_os("SOUND_FORGE_APP") {
        return open(&PathBuf::from(path));
    }
    if Command::new("open").args(["-b", BUNDLE_ID]).status().is_ok_and(|s| s.success()) {
        return Ok(());
    }
    [home().join("Applications").join(APP_NAME), PathBuf::from("/Applications").join(APP_NAME), repo_build()]
        .iter()
        .find(|p| p.exists())
        .map_or_else(|| Err("App Sound Forge introuvable.".into()), |p| open(p))
}

fn open(path: &PathBuf) -> Result<(), String> {
    match Command::new("open").arg(path).status() {
        Ok(status) if status.success() => Ok(()),
        _ => Err(format!("Impossible d'ouvrir {}.", path.display())),
    }
}

fn read_info() -> Option<Info> {
    let text = std::fs::read_to_string(home().join(".sound-forge").join("app.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some(Info {
        port: u16::try_from(value["port"].as_u64()?).ok()?,
        token: value["token"].as_str()?.to_owned(),
    })
}

fn alive(info: &Info) -> bool {
    request(info.port, "GET", "/health", None, "", Duration::from_secs(2))
        .is_ok_and(|(status, body)| status == 200 && body.contains("sound-forge"))
}

fn post(info: &Info, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
    let body = json!({ "method": method, "params": params }).to_string();
    let (_, text) = request(info.port, "POST", "/rpc", Some(&info.token), &body, timeout)
        .map_err(|e| format!("Sound Forge ne répond pas : {e}. L'app a peut-être été fermée."))?;
    let reply: Value = serde_json::from_str(&text).map_err(|e| format!("Réponse illisible de Sound Forge : {e}"))?;
    if reply["ok"].as_bool() == Some(true) {
        Ok(reply["result"].clone())
    } else {
        Err(reply["error"].as_str().unwrap_or("Erreur inconnue de Sound Forge.").to_owned())
    }
}

fn request(port: u16, method: &str, path: &str, token: Option<&str>, body: &str, timeout: Duration) -> std::io::Result<(u16, String)> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);
    let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let chunked = head.to_ascii_lowercase().contains("transfer-encoding: chunked");
    Ok((status, if chunked { dechunk(rest) } else { rest.to_owned() }))
}

/// Corps HTTP « chunked » remis bout à bout.
fn dechunk(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some((size, tail)) = rest.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else { break };
        if size == 0 || tail.len() < size {
            break;
        }
        out.push_str(&tail[..size]);
        rest = tail[size..].trim_start_matches("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dechunks_bodies() {
        assert_eq!(dechunk("4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n"), "{\"a\":1}");
    }
}
