//! Lightweight HTTP sidecar for the WinUI 3 host.
//! Speaks the same command names as Tauri `invoke`.
//!
//!   POST /invoke  { "cmd": "list_bible_books", "args": {} }
//!   GET  /health
//!
//! Concurrent requests are handled on a thread pool so switching Bibles
//! (chapter load + parallel probes) does not block the whole API.

use asignacion_del_cielo_bible_lib::api_dispatch;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
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
    // Catch panics inside workers so one bad module can't kill the sidecar.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("adc-api panic (contained): {info}");
        default_hook(info);
    }));

    let port: u16 = std::env::var("ADC_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("adc-api: failed to bind {addr}: {e}");
        std::process::exit(1);
    });
    eprintln!("adc-api listening on http://{addr} (threaded)");

    // Simple semaphore: cap concurrent handlers so we don't open 100 sqlite files at once.
    let slots = Arc::new(Mutex::new(0usize));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let slots = Arc::clone(&slots);
                thread::spawn(move || {
                    // Wait for a free slot
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
                        handle_client(stream)
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

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
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
                let body_len = request.len() - header_end;
                if body_len >= cl {
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
    let path = parts.next().unwrap_or("/");

    if method == "OPTIONS" {
        return write_cors_empty(&mut stream);
    }

    if method == "GET" && (path == "/health" || path.starts_with("/health?")) {
        let body = json!({
            "ok": true,
            "service": "adc-api",
            "version": env!("CARGO_PKG_VERSION"),
            "threaded": true
        });
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
        // Contain panics inside dispatch (e.g. unexpected sqlite/module issues)
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
