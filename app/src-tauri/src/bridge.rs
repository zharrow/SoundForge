//! Pont entre le serveur MCP de Claude et l'éditeur de sons.
//!
//! Un serveur HTTP écoute sur 127.0.0.1 (port libre). Chaque requête POST /rpc {method, params}
//! est transmise à la page par l'événement « rpc » ; la page applique l'opération au son
//! ouvert et répond via la commande rpc_reply. Le port et un jeton secret sont écrits dans
//! ~/.sound-forge/app.json, que le serveur MCP lit pour se connecter.

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tiny_http::{Header, Request, Response, Server};

const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const READY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Pending {
    waiting: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    next_id: AtomicU64,
    ready: AtomicBool,
}

impl Pending {
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
}

/// La page signale qu'elle écoute les requêtes de Claude.
#[tauri::command]
pub fn bridge_ready(state: State<Pending>) {
    state.ready.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn rpc_reply(id: u64, response: Value, state: State<Pending>) {
    if let Some(sender) = state.waiting.lock().unwrap().remove(&id) {
        let _ = sender.send(response);
    }
}

pub fn start(app: AppHandle) -> Result<(), Box<dyn Error>> {
    let server = Server::http("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|address| address.port())
        .ok_or("adresse du pont introuvable")?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    write_info(port, &token)?;

    thread::spawn(move || {
        for request in server.incoming_requests() {
            let app = app.clone();
            let token = token.clone();
            thread::spawn(move || handle(&app, &token, port, request));
        }
    });
    Ok(())
}

pub fn cleanup() {
    let path = info_path();
    let ours = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|info| info["pid"].as_u64())
        == Some(std::process::id() as u64);
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

fn info_path() -> PathBuf {
    crate::home_dir().join(".sound-forge").join("app.json")
}

fn write_info(port: u16, token: &str) -> std::io::Result<()> {
    let path = info_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let info = json!({ "port": port, "token": token, "pid": std::process::id() });
    std::fs::write(&path, info.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn handle(app: &AppHandle, token: &str, port: u16, mut request: Request) {
    let host_ok = header(&request, "Host")
        .map(|host| host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}"))
        .unwrap_or(false);
    if !host_ok {
        return respond(request, 403, json!({ "ok": false, "error": "Hôte refusé." }));
    }
    if request.url() == "/health" {
        return respond(request, 200, json!({ "ok": true, "app": "sound-forge" }));
    }
    if request.url() != "/rpc" || request.method() != &tiny_http::Method::Post {
        return respond(request, 404, json!({ "ok": false, "error": "Introuvable." }));
    }
    if header(&request, "Authorization") != Some(format!("Bearer {token}")) {
        return respond(request, 401, json!({ "ok": false, "error": "Jeton invalide." }));
    }

    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return respond(request, 400, json!({ "ok": false, "error": "Corps illisible." }));
    }
    let call: Value = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(error) => return respond(request, 400, json!({ "ok": false, "error": error.to_string() })),
    };

    let reply = forward(app, call);
    respond(request, 200, reply);
}

/// Transmet l'appel à la page et attend sa réponse.
fn forward(app: &AppHandle, call: Value) -> Value {
    let state = app.state::<Pending>();
    let started = Instant::now();
    while !state.is_ready() {
        if started.elapsed() > READY_TIMEOUT {
            return json!({ "ok": false, "error": "L'éditeur n'est pas encore prêt." });
        }
        thread::sleep(Duration::from_millis(50));
    }

    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    let (sender, receiver) = mpsc::channel();
    state.waiting.lock().unwrap().insert(id, sender);
    let payload = json!({ "id": id, "method": call["method"], "params": call["params"] });
    if let Err(error) = app.emit_to("main", "rpc", payload) {
        state.waiting.lock().unwrap().remove(&id);
        return json!({ "ok": false, "error": error.to_string() });
    }
    match receiver.recv_timeout(REPLY_TIMEOUT) {
        Ok(reply) => reply,
        Err(_) => {
            state.waiting.lock().unwrap().remove(&id);
            json!({ "ok": false, "error": "L'éditeur n'a pas répondu à temps." })
        }
    }
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str().to_owned())
}

fn respond(request: Request, status: u16, body: Value) {
    let content_type = Header::from_bytes("Content-Type", "application/json").unwrap();
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(content_type);
    let _ = request.respond(response);
}
