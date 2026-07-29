//! Shared command dispatcher used by Tauri and the WinUI HTTP sidecar.
//! JS passes camelCase args (same as Tauri withGlobalTauri).

use crate::esword::{
    get_chapter, get_commentaries, list_books, list_dictionary_topics, lookup_dictionary,
    lookup_lexicon, probe_commentary, probe_dictionary, probe_lexicon, resolve_word_strongs,
    scan_modules, search_commentaries, ContentProbe, DEFAULT_ESWORD_PATH,
};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
    offline: bool,
    mode: String,
    esword_path: String,
    host: String,
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| {
        if v.is_null() {
            None
        } else {
            v.as_str().map(|s| s.to_string())
        }
    })
}

fn arg_i32(args: &Value, key: &str) -> Option<i32> {
    args.get(key).and_then(|v| {
        if let Some(n) = v.as_i64() {
            Some(n as i32)
        } else if let Some(n) = v.as_f64() {
            Some(n as i32)
        } else if let Some(s) = v.as_str() {
            s.parse().ok()
        } else {
            None
        }
    })
}

fn arg_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn arg_str_vec(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn probe_one(
    kind: &str,
    path: String,
    book_number: i32,
    chapter: i32,
    verse: i32,
    term: &str,
    include_verse: bool,
    include_chapter: bool,
    include_book: bool,
) -> ContentProbe {
    match kind {
        "commentary" => probe_commentary(
            &path,
            book_number,
            chapter,
            verse,
            include_verse,
            include_chapter,
            include_book,
        ),
        "dictionary" => probe_dictionary(&path, term),
        "lexicon" => probe_lexicon(&path, term),
        _ => ContentProbe {
            path,
            has_content: false,
            level: String::new(),
            note: "unknown".into(),
        },
    }
}

/// Dispatch a front-end command by name. Returns JSON-serializable result or error string.
pub fn dispatch(cmd: &str, args: &Value) -> Result<Value, String> {
    match cmd {
        "app_info" => {
            let info = AppInfo {
                name: "Asignación del Cielo Bible".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                offline: true,
                mode: "e-sword-local".into(),
                esword_path: DEFAULT_ESWORD_PATH.into(),
                host: "winui3-api".into(),
            };
            serde_json::to_value(info).map_err(|e| e.to_string())
        }
        "list_esword_modules" => {
            let path = arg_str(args, "path");
            let mods = scan_modules(path)?;
            serde_json::to_value(mods).map_err(|e| e.to_string())
        }
        "list_bible_books" => {
            let books = list_books();
            serde_json::to_value(books).map_err(|e| e.to_string())
        }
        "get_bible_chapter" => {
            let module_path = arg_str(args, "modulePath")
                .ok_or_else(|| "modulePath required".to_string())?;
            let book_number =
                arg_i32(args, "bookNumber").ok_or_else(|| "bookNumber required".to_string())?;
            let chapter =
                arg_i32(args, "chapter").ok_or_else(|| "chapter required".to_string())?;
            let res = get_chapter(&module_path, book_number, chapter)?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "get_verse_commentaries" => {
            let module_path = arg_str(args, "modulePath")
                .ok_or_else(|| "modulePath required".to_string())?;
            let book_number =
                arg_i32(args, "bookNumber").ok_or_else(|| "bookNumber required".to_string())?;
            let chapter =
                arg_i32(args, "chapter").ok_or_else(|| "chapter required".to_string())?;
            let verse = arg_i32(args, "verse").ok_or_else(|| "verse required".to_string())?;
            let include_verse = arg_bool(args, "includeVerse", true);
            let include_chapter = arg_bool(args, "includeChapter", false);
            let include_book = arg_bool(args, "includeBook", false);
            let res = get_commentaries(
                &module_path,
                book_number,
                chapter,
                verse,
                include_verse,
                include_chapter,
                include_book,
            )?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "search_dictionary" => {
            let module_path = arg_str(args, "modulePath")
                .ok_or_else(|| "modulePath required".to_string())?;
            let term = arg_str(args, "term").ok_or_else(|| "term required".to_string())?;
            let limit = arg_i32(args, "limit").unwrap_or(20);
            let res = lookup_dictionary(&module_path, &term, limit)?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "search_lexicon" => {
            let module_path = arg_str(args, "modulePath")
                .ok_or_else(|| "modulePath required".to_string())?;
            let term = arg_str(args, "term").ok_or_else(|| "term required".to_string())?;
            let limit = arg_i32(args, "limit").unwrap_or(20);
            let res = lookup_lexicon(&module_path, &term, limit)?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "search_commentary_term" => {
            let module_path = arg_str(args, "modulePath")
                .ok_or_else(|| "modulePath required".to_string())?;
            let term = arg_str(args, "term").ok_or_else(|| "term required".to_string())?;
            let limit = arg_i32(args, "limit").unwrap_or(30);
            let res = search_commentaries(&module_path, &term, limit)?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "suggest_dictionary_topics" => {
            let module_path = arg_str(args, "modulePath")
                .ok_or_else(|| "modulePath required".to_string())?;
            let term = arg_str(args, "term").ok_or_else(|| "term required".to_string())?;
            let limit = arg_i32(args, "limit").unwrap_or(40);
            let res = list_dictionary_topics(&module_path, &term, limit)?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "resolve_strongs" => {
            let book_number =
                arg_i32(args, "bookNumber").ok_or_else(|| "bookNumber required".to_string())?;
            let chapter =
                arg_i32(args, "chapter").ok_or_else(|| "chapter required".to_string())?;
            let verse = arg_i32(args, "verse").ok_or_else(|| "verse required".to_string())?;
            let word = arg_str(args, "word").ok_or_else(|| "word required".to_string())?;
            let word_index = arg_i32(args, "wordIndex").unwrap_or(-1);
            let interlinear_paths = arg_str_vec(args, "interlinearPaths");
            let res = resolve_word_strongs(
                book_number,
                chapter,
                verse,
                &word,
                word_index,
                &interlinear_paths,
            )?;
            serde_json::to_value(res).map_err(|e| e.to_string())
        }
        "probe_modules_content" => {
            let kind = arg_str(args, "kind").unwrap_or_default();
            let paths = arg_str_vec(args, "paths");
            let book_number = arg_i32(args, "bookNumber").unwrap_or(1);
            let chapter = arg_i32(args, "chapter").unwrap_or(1);
            let verse = arg_i32(args, "verse").unwrap_or(1);
            let term = arg_str(args, "term").unwrap_or_default();
            let include_verse = arg_bool(args, "includeVerse", true);
            let include_chapter = arg_bool(args, "includeChapter", false);
            let include_book = arg_bool(args, "includeBook", false);

            let path_count = paths.len();
            let out: Vec<ContentProbe> = if path_count <= 1 {
                paths
                    .into_iter()
                    .map(|p| {
                        probe_one(
                            &kind,
                            p,
                            book_number,
                            chapter,
                            verse,
                            &term,
                            include_verse,
                            include_chapter,
                            include_book,
                        )
                    })
                    .collect()
            } else {
                let workers = std::thread::available_parallelism()
                    .map(|n| n.get().min(8).max(2))
                    .unwrap_or(4);
                let chunk_size = (path_count + workers - 1) / workers;
                let mut handles = Vec::new();
                for chunk in paths.chunks(chunk_size.max(1)) {
                    let chunk: Vec<String> = chunk.to_vec();
                    let kind = kind.clone();
                    let term = term.clone();
                    handles.push(std::thread::spawn(move || {
                        chunk
                            .into_iter()
                            .map(|p| {
                                probe_one(
                                    &kind,
                                    p,
                                    book_number,
                                    chapter,
                                    verse,
                                    &term,
                                    include_verse,
                                    include_chapter,
                                    include_book,
                                )
                            })
                            .collect::<Vec<_>>()
                    }));
                }
                let mut collected = Vec::with_capacity(path_count);
                for h in handles {
                    if let Ok(part) = h.join() {
                        collected.extend(part);
                    }
                }
                collected
            };
            serde_json::to_value(out).map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown command: {other}")),
    }
}

/// JSON envelope for the HTTP sidecar: `{ "ok": true, "result": ... }` or `{ "ok": false, "error": "..." }`.
pub fn dispatch_envelope(cmd: &str, args: &Value) -> Value {
    match dispatch(cmd, args) {
        Ok(result) => json!({ "ok": true, "result": result }),
        Err(error) => json!({ "ok": false, "error": error }),
    }
}
