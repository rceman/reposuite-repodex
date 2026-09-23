//! Minimal loopback HTTP/1.1 server for RepoDex Service V1 (§59).
//!
//! Dependency-free: `TcpListener` + a bounded worker pool, one request per
//! connection (`Connection: close`). Business logic lives behind a `Handler`
//! so a future Unix-socket/named-pipe/in-process transport can reuse it (§60).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct Request {
    pub method: String,
    pub path: String,
    pub bearer: Option<String>,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub body: String,
}
impl Response {
    pub fn json(status: u16, v: &serde_json::Value) -> Self {
        Self {
            status,
            body: v.to_string(),
        }
    }
    pub fn err(status: u16, code: &str, msg: &str) -> Self {
        Self::json(
            status,
            &serde_json::json!({"error":{"code":code,"message":msg}}),
        )
    }
}

pub type Handler = Arc<dyn Fn(&Request) -> Response + Send + Sync>;

pub struct HttpServer {
    listener: TcpListener,
    stop: Arc<AtomicBool>,
}

impl HttpServer {
    /// Bind host:port (port 0 = OS-assigned); report the actual port (§9-§10).
    pub fn bind(host: &str, port: u16) -> Result<Self, String> {
        let listener =
            TcpListener::bind((host, port)).map_err(|e| format!("bind {host}:{port}: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self {
            listener,
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn local_addr(&self) -> Result<(String, u16), String> {
        self.listener
            .local_addr()
            .map(|a| (a.ip().to_string(), a.port()))
            .map_err(|e| e.to_string())
    }

    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Serve until `stop` is set. Bounded worker pool; connections queue on a
    /// channel so a burst can't spawn unbounded threads (§43, §54).
    pub fn run(&self, handler: Handler, workers: usize) {
        let (tx, rx) = channel::<TcpStream>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..workers.max(4) {
            let rx = rx.clone();
            let h = handler.clone();
            thread::spawn(move || loop {
                let stream = {
                    let rx = rx.lock().unwrap();
                    rx.recv()
                };
                match stream {
                    Ok(s) => handle_conn(s, &h),
                    Err(_) => break,
                }
            });
        }
        while !self.stop.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((s, _)) => {
                    let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
                    let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
                    if tx.send(s).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    }
}

fn handle_conn(mut s: TcpStream, h: &Handler) {
    let req = match read_request(&mut s) {
        Ok(r) => r,
        Err(_) => {
            let _ = write_response(
                &mut s,
                &Response::err(400, "BAD_REQUEST", "malformed request"),
            );
            return;
        }
    };
    let res = h(&req);
    let _ = write_response(&mut s, &res);
}

fn read_request(s: &mut TcpStream) -> Result<Request, String> {
    let mut buf = Vec::with_capacity(8192);
    let mut tmp = [0u8; 8192];
    // Read until headers complete.
    let header_end;
    loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > 64 * 1024 {
            return Err("headers too large".into());
        }
        let n = s.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("eof before headers".into());
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let reqline = lines.next().ok_or("no request line")?;
    let mut parts = reqline.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let path = parts
        .next()
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let mut content_len = 0usize;
    let mut bearer = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            if k == "content-length" {
                content_len = v.parse().unwrap_or(0);
            } else if k == "authorization" {
                if let Some(t) = v.strip_prefix("Bearer ") {
                    bearer = Some(t.trim().to_string());
                } else {
                    bearer = Some(String::new()); // malformed auth -> reject later
                }
            }
        }
    }
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_len {
        let n = s.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);
    Ok(Request {
        method,
        path,
        bearer,
        body,
    })
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn write_response(s: &mut TcpStream, r: &Response) -> Result<(), String> {
    let reason = match r.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        r.status,
        reason,
        r.body.len()
    );
    s.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    s.write_all(r.body.as_bytes()).map_err(|e| e.to_string())?;
    s.flush().map_err(|e| e.to_string())
}

/// Client helper used by `stop`/`status`/`query --service` (uses `ureq`).
pub fn post_json(
    host: &str,
    port: u16,
    path: &str,
    token: Option<&str>,
    body: &serde_json::Value,
    timeout_ms: u64,
) -> Result<(u16, serde_json::Value), String> {
    let url = format!("http://{host}:{port}{path}");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(timeout_ms))
        .build();
    let mut req = agent.post(&url).set("Content-Type", "application/json");
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let payload = body.to_string();
    match req.send_string(&payload) {
        Ok(r) => {
            let st = r.status();
            let v = parse_body(r);
            Ok((st, v))
        }
        Err(ureq::Error::Status(st, r)) => Ok((st, parse_body(r))),
        Err(e) => Err(e.to_string()),
    }
}

pub fn get_json(
    host: &str,
    port: u16,
    path: &str,
    token: Option<&str>,
    timeout_ms: u64,
) -> Result<(u16, serde_json::Value), String> {
    let url = format!("http://{host}:{port}{path}");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(timeout_ms))
        .build();
    let mut req = agent.get(&url);
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    match req.call() {
        Ok(r) => {
            let st = r.status();
            let v = parse_body(r);
            Ok((st, v))
        }
        Err(ureq::Error::Status(st, r)) => Ok((st, parse_body(r))),
        Err(e) => Err(e.to_string()),
    }
}

fn parse_body(r: ureq::Response) -> serde_json::Value {
    r.into_string()
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// Unused — keeps Receiver type referenced for future streaming.
#[allow(dead_code)]
type _Rx = Receiver<TcpStream>;
#[allow(dead_code)]
type _Tx = Sender<TcpStream>;
