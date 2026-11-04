//! Local receiver. POST /speak with `Authorization: Bearer <token>` and body
//! {"text": "..."}. Nothing but the reply text is accepted or kept.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;

use serde::Deserialize;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::speaker::Speaker;
use crate::status::{Bridge, Shared};

pub const MAX_TEXT_BYTES: usize = 8000;
pub const MAX_BODY_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
struct Payload {
    text: String,
}

pub struct Handle {
    server: Arc<Server>,
    pub addr: SocketAddr,
}

impl Handle {
    pub fn stop(&self) {
        self.server.unblock();
    }
}

pub fn start(
    addr: SocketAddr,
    token: String,
    speaker: Arc<Speaker>,
    shared: Shared,
) -> Result<Handle, String> {
    if !addr.ip().is_loopback() {
        return Err("Receiver only listens on this computer".into());
    }
    if token.is_empty() {
        return Err("Set an access token first".into());
    }
    let server = Server::http(addr).map_err(|e| format!("Could not listen on {addr}: {e}"))?;
    let real = server.server_addr().to_ip().ok_or("Unexpected address")?;
    let server = Arc::new(server);
    shared.update(|s| {
        if s.bridge != Bridge::Connected {
            s.bridge = Bridge::Listening;
        }
    });
    let srv = server.clone();
    std::thread::spawn(move || {
        for req in srv.incoming_requests() {
            handle(req, &token, &speaker, &shared);
        }
    });
    Ok(Handle { server, addr: real })
}

fn json(code: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_status_code(code)
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

pub fn token_matches(given: &str, expected: &str) -> bool {
    let a = given.as_bytes();
    let b = expected.as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (*a.get(i).unwrap_or(&0) ^ *b.get(i).unwrap_or(&0)) as usize;
    }
    diff == 0
}

fn bearer(req: &Request) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .and_then(|h| {
            h.value
                .as_str()
                .strip_prefix("Bearer ")
                .map(|s| s.to_string())
        })
}

fn handle(mut req: Request, token: &str, speaker: &Speaker, shared: &Shared) {
    let path = req.url().split('?').next().unwrap_or("").to_string();
    let method = req.method().clone();
    // Health check carries no data and needs no token.
    if method == Method::Get && path == "/health" {
        let _ = req.respond(json(200, r#"{"ok":true}"#));
        return;
    }
    if path != "/speak" {
        let _ = req.respond(json(404, r#"{"error":"not found"}"#));
        return;
    }
    if method != Method::Post {
        let _ = req.respond(json(405, r#"{"error":"use POST"}"#));
        return;
    }
    match bearer(&req) {
        Some(t) if token_matches(&t, token) => {}
        _ => {
            let _ = req.respond(json(401, r#"{"error":"bad token"}"#));
            return;
        }
    }
    if req
        .body_length()
        .map(|n| n > MAX_BODY_BYTES)
        .unwrap_or(false)
    {
        let _ = req.respond(json(413, r#"{"error":"too large"}"#));
        return;
    }
    let mut body = String::new();
    {
        use std::io::Read;
        let mut limited = req.as_reader().take(MAX_BODY_BYTES as u64 + 1);
        if limited.read_to_string(&mut body).is_err() || body.len() > MAX_BODY_BYTES {
            let _ = req.respond(json(413, r#"{"error":"too large or not text"}"#));
            return;
        }
    }
    let text = match serde_json::from_str::<Payload>(&body) {
        Ok(p) => p.text,
        Err(_) => {
            let _ = req.respond(json(400, r#"{"error":"expected {\"text\": \"...\"}"}"#));
            return;
        }
    };
    let text = text.trim().to_string();
    if text.is_empty() || text.len() > MAX_TEXT_BYTES {
        let _ = req.respond(json(400, r#"{"error":"empty or too long"}"#));
        return;
    }
    // Right token and a real payload: this is the first proof the route works.
    shared.update(|s| {
        s.bridge = Bridge::Connected;
        s.last_reply_at = Some(SystemTime::now());
        s.last_reply = Some(text.clone());
    });
    if speaker.enqueue(text).is_err() {
        let _ = req.respond(json(429, r#"{"error":"busy, try again"}"#));
        return;
    }
    let _ = req.respond(json(202, r#"{"queued":true}"#));
}
