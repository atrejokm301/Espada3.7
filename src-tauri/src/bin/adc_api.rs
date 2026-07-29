//! Lightweight HTTP sidecar for the WinUI 3 host.
//! Speaks the same command names as Tauri `invoke`.
//!
//!   POST /invoke  { "cmd": "list_bible_books", "args": {} }
//!   GET  /health

use asignacion_del_cielo_bible_lib::api_dispatch;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const DEFAULT_PORT: u16 = 17865;

#[derive(Deserialize)]
struct InvokeBody {
    cmd: String,
    #[serde(default)]
    args: Value,
}

fn main() {
    let port: u16 = std::env::var("ADC_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("adc-api: failed to bind {addr}: {e}");
        std::process::exit(1);
    });
    // Allow concurrent short requests without hanging accept forever.
    let _ = listener.set_nonblocking(false);
    eprintln!("adc-api listening on http://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle_client(stream) {
                    eprintln!("adc-api request error: {e}");
                }
            }
            Err(e) => eprintln!("adc-api accept error: {e}"),
        }
    }
}

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let mut buf = vec![0u8; 65536];
    let mut request = Vec::new();
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") {
            // If Content-Length present, keep reading until body is complete.
            if let Some(cl) = content_length(&request) {
                if let Some(header_end) = find_header_end(&request) {
                    let body_len = request.len() - header_end;
                    if body_len >= cl {
                        break;
                    }
                    // grow and continue
                    if request.capacity() < header_end + cl {
                        request.reserve(header_end + cl - request.len());
                    }
                    continue;
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
    let path = parts.next().unwrap_or("/");

    // CORS preflight for https://app.local WebView mapping
    if method == "OPTIONS" {
        return write_cors_empty(&mut stream);
    }

    if method == "GET" && (path == "/health" || path.starts_with("/health?")) {
        let body = json!({ "ok": true, "service": "adc-api", "version": env!("CARGO_PKG_VERSION") });
        return write_json(&mut stream, 200, &body);
    }

    if method == "POST" && (path == "/invoke" || path.starts_with("/invoke?")) {
        let body_bytes = &request[header_end..];
        let invoke: InvokeBody = match serde_json::from_slice(body_bytes) {
            Ok(v) => v,
            Err(e) => {
                let body = json!({ "ok": false, "error": format!("invalid JSON body: {e}") });
                return write_json(&mut stream, 400, &body);
            }
        };
        let envelope = api_dispatch::dispatch_envelope(&invoke.cmd, &invoke.args);
        return write_json(&mut stream, 200, &envelope);
    }

    write_json(
        &mut stream,
        404,
        &json!({ "ok": false, "error": "not found" }),
    )
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
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         {}\
         Connection: close\r\n\r\n",
        body.len(),
        cors_headers()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}
