//! HTTP backend for Espada 3.7 (Windows).
//!
//! - POST /invoke  — same commands as Tauri
//! - GET  /health
//! - GET  /*       — serves the study UI (static files) so we can run without
//!                   WinUI/WebView2 when those hosts crash
//!
//! Env:
//!   ADC_API_PORT  (default 17865)
//!   ADC_UI_DIR    (folder containing index.html)

use asignacion_del_cielo_bible_lib::api_dispatch;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_PORT: u16 = 17865;
const MAX_WORKERS: usize = 8;

#[derive(Deserialize)]
struct InvokeBody {
    cmd: String,
    #[serde(default)]
    args: Value,
}

fn main() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("adc-api panic (contained): {info}");
        default_hook(info);
    }));

    let port: u16 = std::env::var("ADC_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let ui_dir = resolve_ui_dir();
    if let Some(ref d) = ui_dir {
        eprintln!("adc-api UI dir: {}", d.display());
    } else {
        eprintln!("adc-api UI dir: (none — API only; set ADC_UI_DIR to serve the study UI)");
    }

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("adc-api: failed to bind {addr}: {e}");
        std::process::exit(1);
    });
    eprintln!("adc-api listening on http://{addr}");
    eprintln!("  open study UI: http://{addr}/");

    let slots = Arc::new(Mutex::new(0usize));
    let ui_dir = Arc::new(ui_dir);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let slots = Arc::clone(&slots);
                let ui_dir = Arc::clone(&ui_dir);
                thread::spawn(move || {
                    loop {
                        {
                            let mut n = slots.lock().unwrap_or_else(|e| e.into_inner());
                            if *n < MAX_WORKERS {
                                *n += 1;
                                break;
                            }
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handle_client(stream, ui_dir.as_ref().as_ref())
                    }));
                    {
                        let mut n = slots.lock().unwrap_or_else(|e| e.into_inner());
                        *n = n.saturating_sub(1);
                    }
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => eprintln!("adc-api request error: {e}"),
                        Err(_) => eprintln!("adc-api: request handler panicked"),
                    }
                });
            }
            Err(e) => eprintln!("adc-api accept error: {e}"),
        }
    }
}

fn resolve_ui_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ADC_UI_DIR") {
        let pb = PathBuf::from(p);
        if pb.join("index.html").is_file() {
            return Some(pb);
        }
    }
    // Walk from exe and cwd looking for ui/index.html
    let mut seeds = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            seeds.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        seeds.push(cwd);
    }
    for mut dir in seeds {
        for _ in 0..10 {
            let candidate = dir.join("ui");
            if candidate.join("index.html").is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    None
}

fn handle_client(mut stream: TcpStream, ui_dir: Option<&PathBuf>) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));

    let mut buf = vec![0u8; 65536];
    let mut request = Vec::new();
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => return Err(e),
        };
        request.extend_from_slice(&buf[..n]);
        if let Some(header_end) = find_header_end(&request) {
            if let Some(cl) = content_length(&request) {
                if request.len() - header_end >= cl {
                    break;
                }
            } else {
                break;
            }
        }
        if request.len() > 16 * 1024 * 1024 {
            return write_response(&mut stream, 413, "text/plain", b"payload too large");
        }
    }

    if request.is_empty() {
        return Ok(());
    }

    let header_end = match find_header_end(&request) {
        Some(i) => i,
        None => return write_response(&mut stream, 400, "text/plain", b"bad request"),
    };
    let header = String::from_utf8_lossy(&request[..header_end]);
    let first_line = header.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_path = parts.next().unwrap_or("/");
    // strip query
    let path = raw_path.split('?').next().unwrap_or("/");

    if method == "OPTIONS" {
        return write_cors_empty(&mut stream);
    }

    if method == "GET" && (path == "/health" || path.starts_with("/health")) {
        let body = json!({
            "ok": true,
            "service": "adc-api",
            "version": env!("CARGO_PKG_VERSION"),
            "threaded": true,
            "ui": ui_dir.is_some(),
        });
        return write_json(&mut stream, 200, &body);
    }

    if method == "POST" && (path == "/invoke" || path.starts_with("/invoke")) {
        let body_bytes = &request[header_end..];
        let invoke: InvokeBody = match serde_json::from_slice(body_bytes) {
            Ok(v) => v,
            Err(e) => {
                let body = json!({ "ok": false, "error": format!("invalid JSON body: {e}") });
                return write_json(&mut stream, 400, &body);
            }
        };
        let envelope = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            api_dispatch::dispatch_envelope(&invoke.cmd, &invoke.args)
        })) {
            Ok(v) => v,
            Err(_) => json!({
                "ok": false,
                "error": format!("internal panic while handling '{}'", invoke.cmd)
            }),
        };
        return write_json(&mut stream, 200, &envelope);
    }

    if method == "GET" {
        if let Some(root) = ui_dir {
            return serve_static(&mut stream, root, path);
        }
    }

    write_json(
        &mut stream,
        404,
        &json!({ "ok": false, "error": "not found" }),
    )
}

fn serve_static(stream: &mut TcpStream, root: &Path, url_path: &str) -> std::io::Result<()> {
    let rel = if url_path == "/" || url_path.is_empty() {
        "index.html"
    } else {
        url_path.trim_start_matches('/')
    };

    // Block path traversal
    if rel.contains("..") || rel.contains('\\') {
        return write_response(stream, 400, "text/plain", b"bad path");
    }

    let full = root.join(rel);
    let full = match full.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return write_response(stream, 404, "text/plain", b"not found");
        }
    };
    let root_can = match root.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return write_response(stream, 500, "text/plain", b"ui root error");
        }
    };
    if !full.starts_with(&root_can) || !full.is_file() {
        return write_response(stream, 404, "text/plain", b"not found");
    }

    let mut bytes = std::fs::read(&full)?;
    let ctype = content_type_for(&full);

    // Inject host bridge into the main HTML so invoke works without WinUI.
    if full.file_name().and_then(|n| n.to_str()) == Some("index.html") {
        if let Ok(html) = String::from_utf8(bytes.clone()) {
            let inject = r#"<script>
window.__ADC_HOST__ = window.__ADC_HOST__ || 'edge-app';
window.__ADC_API_BASE__ = window.__ADC_API_BASE__ || (location.origin);
console.info('[ADC] edge-app host bridge', window.__ADC_API_BASE__);
</script>"#;
            let injected = if let Some(i) = html.find("<head>") {
                let mut s = String::with_capacity(html.len() + inject.len() + 8);
                s.push_str(&html[..i + 6]);
                s.push('\n');
                s.push_str(inject);
                s.push_str(&html[i + 6..]);
                s
            } else {
                format!("{inject}{html}")
            };
            bytes = injected.into_bytes();
        }
    }

    write_response(stream, 200, ctype, &bytes)
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}

fn content_length(request: &[u8]) -> Option<usize> {
    let header_end = find_header_end(request)?;
    let header = std::str::from_utf8(&request[..header_end]).ok()?;
    for line in header.lines() {
        if let Some(rest) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn cors_headers() -> &'static str {
    "Access-Control-Allow-Origin: *\r\n\
     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
     Access-Control-Allow-Headers: Content-Type\r\n\
     Access-Control-Max-Age: 86400\r\n"
}

fn write_cors_empty(stream: &mut TcpStream) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 204 No Content\r\n{}Connection: close\r\n\r\n",
        cors_headers()
    );
    stream.write_all(resp.as_bytes())
}

fn write_json(stream: &mut TcpStream, status: u16, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    write_response(stream, status, "application/json; charset=utf-8", &body)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-cache\r\n\
         {}\
         Connection: close\r\n\r\n",
        body.len(),
        cors_headers()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}
