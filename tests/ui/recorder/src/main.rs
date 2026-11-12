//! Records real-backend responses for the UI test harness (tests/ui/harness.js).
//!
//! Usage: recorder <fixture.json> <label> <requests.json>...
//!   requests.json : [{ "key": "...", "cmd": "...", "args": {...} }, ...]  (written by harness collect mode)
//!   fixture.json  : merged in place: { backend, signatures, responses: { key: {ok}|{err} } }
//!
//! The command bodies below mirror the thin wrappers in src-tauri/src/lib.rs (HEAD 6748155),
//! calling the same module functions the app calls. record.sh copies modules.rs / text.rs /
//! books.rs / crossrefs.rs from the chosen backend source next to this file before building.
#![allow(dead_code, unused_imports)]

mod books;
mod crossrefs;
mod modules;
mod text;

use modules::*;
use serde_json::{json, Map, Value};
use std::fs;

fn s(a: &Value, k: &str) -> String {
    a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
}
fn os(a: &Value, k: &str) -> Option<String> {
    a.get(k).and_then(|v| v.as_str()).map(|x| x.to_string())
}
fn i(a: &Value, k: &str) -> i32 {
    a.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as i32
}
fn oi(a: &Value, k: &str) -> Option<i32> {
    a.get(k).and_then(|v| v.as_i64()).map(|x| x as i32)
}
fn vs(a: &Value, k: &str) -> Vec<String> {
    a.get(k)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}
fn filt(a: &Value, k: &str) -> Option<(bool, bool, bool)> {
    let f = a.get(k)?;
    if f.is_null() {
        return None;
    }
    Some((
        f.get("verse").and_then(|v| v.as_bool()).unwrap_or(false),
        f.get("chapter").and_then(|v| v.as_bool()).unwrap_or(false),
        f.get("book").and_then(|v| v.as_bool()).unwrap_or(false),
    ))
}
fn ok<T: serde::Serialize>(v: T) -> Value {
    json!({ "ok": serde_json::to_value(v).unwrap() })
}
fn res<T: serde::Serialize>(r: Result<T, String>) -> Value {
    match r {
        Ok(v) => ok(v),
        Err(e) => json!({ "err": e }),
    }
}

/// Arg names each command of THIS backend deserializes (Tauri ignores any other key).
fn signatures() -> Value {
    json!({
        "get_books": [],
        "get_default_modules_dir": [],
        "list_modules": ["modulesDir"],
        "refresh_modules": ["modulesDir"],
        "get_bible_chapter": ["modulePath", "book", "chapter"],
        "get_verse_plain": ["modulePath", "book", "chapter", "verse"],
        "get_passage": ["modulePath", "book", "chapter", "verseStart", "verseEnd"],
        "get_verse_count": ["modulePath", "book", "chapter"],
        "get_commentary": ["modulePath", "book", "chapter", "verse", "filters"],
        "lookup_dictionary": ["modulePath", "query", "limit"],
        "lookup_lexicon": ["modulePath", "strong"],
        "lookup_lexicon_multi": ["modulePaths", "strong"],
        "lookup_reference": ["modulePath", "query", "limit"],
        "resolve_word": ["word", "dictPaths", "lexPaths", "interlinearPath", "book", "chapter", "verse"],
        "find_interlinear": ["modulesDir"],
        "probe_modules_content": ["kind", "paths", "book", "chapter", "verse", "query", "filters"],
        "resolve_book_ref": ["raw"],
        "list_cross_reference_sources": ["modulesDir"],
        "get_cross_references": ["modulesDir", "book", "chapter", "verse", "sourcePath", "limit"]
    })
}

fn run(cmd: &str, a: &Value) -> Value {
    match cmd {
        "get_books" => ok(books::books_json()),
        "get_default_modules_dir" => ok(find_default_modules_dir()),
        "list_modules" | "refresh_modules" => res(scan_modules(&s(a, "modulesDir"))),
        "get_bible_chapter" => res(load_bible_chapter(&s(a, "modulePath"), i(a, "book"), i(a, "chapter"))),
        "get_verse_plain" => res(load_verse_text(
            &s(a, "modulePath"),
            i(a, "book"),
            i(a, "chapter"),
            i(a, "verse"),
        )),
        "get_passage" => {
            // Mirrors lib.rs get_passage (without the chapter cache, which does not change output).
            let path = s(a, "modulePath");
            let (book, chapter) = (i(a, "book"), i(a, "chapter"));
            let mut v0 = i(a, "verseStart").max(1);
            let mut v1 = oi(a, "verseEnd").unwrap_or(v0).max(1);
            if v1 < v0 {
                std::mem::swap(&mut v0, &mut v1);
            }
            if v1 - v0 > 40 {
                v1 = v0 + 40;
            }
            match load_bible_chapter(&path, book, chapter) {
                Err(e) => json!({ "err": e }),
                Ok(ch) => {
                    let verses: Vec<Value> = ch
                        .verses
                        .iter()
                        .filter(|v| v.verse >= v0 && v.verse <= v1)
                        .map(|v| json!({"verse": v.verse, "plain": v.plain}))
                        .collect();
                    if verses.is_empty() {
                        match load_verse_text(&path, book, chapter, v0) {
                            Err(e) => json!({ "err": e }),
                            Ok(plain) => ok(json!({"book": book, "chapter": chapter, "verseStart": v0, "verseEnd": v0,
                                "abbreviation": ch.abbreviation, "title": ch.title,
                                "verses": [{"verse": v0, "plain": plain}]})),
                        }
                    } else {
                        let first = verses.first().unwrap()["verse"].clone();
                        let last = verses.last().unwrap()["verse"].clone();
                        ok(json!({"book": book, "chapter": chapter, "verseStart": first, "verseEnd": last,
                            "abbreviation": ch.abbreviation, "title": ch.title, "verses": verses}))
                    }
                }
            }
        }
        "get_verse_count" => res(verse_count(&s(a, "modulePath"), i(a, "book"), i(a, "chapter"))),
        "get_commentary" => {
            let (fv, fc, fb) = filt(a, "filters").unwrap_or((true, false, false));
            res(load_commentary(&s(a, "modulePath"), i(a, "book"), i(a, "chapter"), i(a, "verse"), fv, fc, fb))
        }
        "lookup_dictionary" => res(dictionary_lookup(
            &s(a, "modulePath"),
            &s(a, "query"),
            oi(a, "limit").unwrap_or(30),
        )),
        "lookup_lexicon" => res(lexicon_lookup(&s(a, "modulePath"), &s(a, "strong"))),
        "lookup_lexicon_multi" => {
            let strong = s(a, "strong");
            let mut last_err: Option<String> = None;
            let mut found = None;
            for p in vs(a, "modulePaths") {
                match lexicon_lookup(&p, &strong) {
                    Ok(Some(e)) => {
                        found = Some(e);
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => last_err = Some(e),
                }
            }
            if found.is_none() && last_err.is_some() && strong.trim().is_empty() {
                json!({ "err": last_err.unwrap() })
            } else {
                ok(found)
            }
        }
        "lookup_reference" => res(reference_lookup(
            &s(a, "modulePath"),
            &s(a, "query"),
            oi(a, "limit").unwrap_or(40),
        )),
        "resolve_word" => {
            let inter = os(a, "interlinearPath");
            ok(resolve_word_links(
                &s(a, "word"),
                &vs(a, "dictPaths"),
                &vs(a, "lexPaths"),
                inter.as_deref(),
                i(a, "book"),
                i(a, "chapter"),
                i(a, "verse"),
            ))
        }
        "find_interlinear" => {
            let r = scan_modules(&s(a, "modulesDir")).ok().and_then(|m| find_interlinear_path(&m));
            ok(r)
        }
        "probe_modules_content" => {
            let kind = s(a, "kind");
            let (fv, fc, fb) = filt(a, "filters").unwrap_or((true, false, false));
            let (book, chapter, verse) = (i(a, "book"), i(a, "chapter"), i(a, "verse"));
            let query = s(a, "query");
            let out: Vec<Value> = vs(a, "paths")
                .into_iter()
                .map(|path| {
                    let has = match kind.as_str() {
                        "commentary" => probe_commentary_has(&path, book, chapter, verse, fv, fc, fb),
                        "dictionary" => probe_dictionary_has(&path, &query),
                        "lexicon" => {
                            if query.trim().is_empty() {
                                lexicon_lookup(&path, "G26").map(|v| v.is_some()).unwrap_or(false)
                                    || lexicon_lookup(&path, "H430").map(|v| v.is_some()).unwrap_or(false)
                            } else {
                                probe_lexicon_has(&path, &query)
                            }
                        }
                        "reference" => reference_lookup(&path, &query, 1).map(|v| !v.is_empty()).unwrap_or(false),
                        _ => false,
                    };
                    json!({"path": path, "has": has})
                })
                .collect();
            ok(out)
        }
        "resolve_book_ref" => ok(books::resolve_book_name(&s(a, "raw"))),
        "list_cross_reference_sources" => {
            let dir = s(a, "modulesDir");
            res(scan_modules(&dir).map(|m| crossrefs::list_cross_reference_sources(&m, &dir)))
        }
        "get_cross_references" => {
            let dir = s(a, "modulesDir");
            let src = os(a, "sourcePath");
            res(scan_modules(&dir).and_then(|m| {
                crossrefs::load_cross_references(
                    &m,
                    &dir,
                    i(a, "book"),
                    i(a, "chapter"),
                    i(a, "verse"),
                    src.as_deref(),
                    oi(a, "limit").unwrap_or(80),
                )
            }))
        }
        other => json!({ "err": format!("recorder: unknown command {other}") }),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: recorder <fixture.json> <label> <requests.json>...");
        std::process::exit(2);
    }
    let mut reqs: Vec<Value> = Vec::new();
    for f in &args[3..] {
        let t = fs::read_to_string(f).unwrap_or_default();
        if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&t) {
            reqs.extend(a);
        }
    }
    let mut fixture: Value = fs::read_to_string(&args[1])
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}));
    if !fixture.is_object() {
        fixture = json!({});
    }
    let obj = fixture.as_object_mut().unwrap();
    obj.insert("backend".into(), json!(args[2]));
    obj.insert("signatures".into(), signatures());
    let responses = obj
        .entry("responses")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let mut n = 0;
    for r in reqs {
        let key = r["key"].as_str().unwrap_or("").to_string();
        if key.is_empty() || responses.contains_key(&key) {
            continue;
        }
        let cmd = r["cmd"].as_str().unwrap_or("");
        let out = run(cmd, &r["args"]);
        responses.insert(key, out);
        n += 1;
    }
    // Stable key order keeps fixture diffs reviewable.
    let mut sorted: Vec<(String, Value)> = std::mem::take(responses).into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in sorted {
        responses.insert(k, v);
    }
    fs::write(&args[1], serde_json::to_string_pretty(&fixture).unwrap()).expect("write fixture");
    eprintln!("recorder: {n} new responses -> {}", args[1]);
}
