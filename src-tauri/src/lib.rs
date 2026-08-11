mod esword;
pub mod api_dispatch;

// Shared e-Sword helpers for sibling apps (e.g. e-ink Tauri host).
pub use esword::{
    get_chapter as esword_get_chapter, list_books as esword_list_books,
    scan_modules as esword_scan_modules, BookInfo, ChapterResult, ModuleInfo, DEFAULT_ESWORD_PATH,
};

use esword::{
    get_chapter, get_commentaries, list_books, list_dictionary_topics, lookup_dictionary,
    lookup_lexicon, probe_commentary, probe_dictionary, probe_lexicon, resolve_word_strongs,
    scan_modules, search_bible_word as esword_search_bible_word, search_commentaries,
    BibleSearchHit, CommentaryResult, ContentProbe, DictionaryResult, LexiconResult,
    ResolveStrongsResult,
};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Append-only crash log under %LOCALAPPDATA%\asignacion-del-cielo-bible\
fn crash_log_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))?;
    Some(PathBuf::from(base).join("asignacion-del-cielo-bible").join("tauri-crash.log"))
}

fn write_crash_log(msg: &str) {
    let Some(path) = crash_log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            f,
            "[{}] {}",
            chrono_like_now(),
            msg
        );
    }
}

fn chrono_like_now() -> String {
    // Avoid extra chrono dep — simple local-ish timestamp via system time
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix={secs}")
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".into()
        };
        let msg = format!("PANIC at {loc} — {payload}");
        eprintln!("{msg}");
        write_crash_log(&msg);
        write_crash_log(&format!("note: release profile uses panic=abort; process will exit"));
    }));
}

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

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Espada 3.7".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        offline: true,
        mode: "e-sword-local-windows".into(),
        esword_path: DEFAULT_ESWORD_PATH.into(),
        host: "tauri-windows".into(),
    }
}

#[tauri::command]
fn list_esword_modules(path: Option<String>) -> Result<Vec<ModuleInfo>, String> {
    scan_modules(path)
}

#[tauri::command]
fn list_bible_books() -> Vec<BookInfo> {
    list_books()
}

#[tauri::command]
fn get_bible_chapter(
    module_path: String,
    book_number: i32,
    chapter: i32,
) -> Result<ChapterResult, String> {
    get_chapter(&module_path, book_number, chapter)
}

#[tauri::command]
fn get_verse_commentaries(
    module_path: String,
    book_number: i32,
    chapter: i32,
    verse: i32,
    include_verse: Option<bool>,
    include_chapter: Option<bool>,
    include_book: Option<bool>,
) -> Result<Vec<CommentaryResult>, String> {
    // Defaults: verse-only isolation (no book intro dumping into empty verses)
    get_commentaries(
        &module_path,
        book_number,
        chapter,
        verse,
        include_verse.unwrap_or(true),
        include_chapter.unwrap_or(false),
        include_book.unwrap_or(false),
    )
}

#[tauri::command]
fn search_dictionary(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<DictionaryResult>, String> {
    lookup_dictionary(&module_path, &term, limit.unwrap_or(20))
}

#[tauri::command]
fn search_lexicon(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<LexiconResult>, String> {
    lookup_lexicon(&module_path, &term, limit.unwrap_or(20))
}

#[tauri::command]
fn search_commentary_term(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<CommentaryResult>, String> {
    search_commentaries(&module_path, &term, limit.unwrap_or(30))
}

/// Concordance / word cross-references in the selected Bible module.
#[tauri::command]
fn search_bible_word(
    module_path: String,
    term: String,
    exclude_book: Option<i32>,
    exclude_chapter: Option<i32>,
    exclude_verse: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<BibleSearchHit>, String> {
    esword_search_bible_word(
        &module_path,
        &term,
        exclude_book,
        exclude_chapter,
        exclude_verse,
        limit.unwrap_or(40),
    )
}

#[tauri::command]
fn suggest_dictionary_topics(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<DictionaryResult>, String> {
    list_dictionary_topics(&module_path, &term, limit.unwrap_or(40))
}

#[tauri::command]
fn resolve_strongs(
    book_number: i32,
    chapter: i32,
    verse: i32,
    word: String,
    word_index: Option<i32>,
    interlinear_paths: Vec<String>,
) -> Result<ResolveStrongsResult, String> {
    resolve_word_strongs(
        book_number,
        chapter,
        verse,
        &word,
        word_index.unwrap_or(-1),
        &interlinear_paths,
    )
}

#[tauri::command]
fn probe_modules_content(
    kind: String,
    paths: Vec<String>,
    book_number: i32,
    chapter: i32,
    verse: i32,
    term: Option<String>,
    include_verse: Option<bool>,
    include_chapter: Option<bool>,
    include_book: Option<bool>,
) -> Vec<ContentProbe> {
    let term = term.unwrap_or_default();
    let include_verse = include_verse.unwrap_or(true);
    let include_chapter = include_chapter.unwrap_or(false);
    let include_book = include_book.unwrap_or(false);
    // Hard cap: after mass-import, probing 200+ modules at once can freeze/OOM.
    const MAX_PROBE_PATHS: usize = 48;
    let paths: Vec<String> = paths.into_iter().take(MAX_PROBE_PATHS).collect();
    // Parallel probes: different modules open concurrently (conn cache is per-path).
    if paths.len() <= 1 {
        return paths
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
            .collect();
    }
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(4).max(2))
        .unwrap_or(2);
    let chunk_size = (paths.len() + workers - 1) / workers;
    let mut handles = Vec::new();
    for chunk in paths.chunks(chunk_size.max(1)) {
        let chunk: Vec<String> = chunk.to_vec();
        let kind = kind.clone();
        let term = term.clone();
        handles.push(std::thread::spawn(move || {
            chunk
                .into_iter()
                .map(|p| {
                    // Never let one bad module panic the worker thread
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        probe_one(
                            &kind,
                            p.clone(),
                            book_number,
                            chapter,
                            verse,
                            &term,
                            include_verse,
                            include_chapter,
                            include_book,
                        )
                    }))
                    .unwrap_or_else(|_| ContentProbe {
                        path: p,
                        has_content: false,
                        level: String::new(),
                        note: "probe panic".into(),
                    })
                })
                .collect::<Vec<_>>()
        }));
    }
    let mut out = Vec::with_capacity(paths.len());
    for h in handles {
        match h.join() {
            Ok(part) => out.extend(part),
            Err(_) => {
                write_crash_log("probe worker thread panicked (join err)");
            }
        }
    }
    out
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

/// Toggle Windows acrylic/blur so themed glass can bleed wallpaper / apps behind the window.
/// Optional `r,g,b,a` tint the acrylic wash to match the selected theme.
/// Requires `transparent: true` on the window (see tauri.conf.json).
/// Soft-fails: never panics — GPU/driver quirks must not take down the app.
#[tauri::command]
fn set_window_glass(
    window: tauri::WebviewWindow,
    enabled: bool,
    r: Option<u8>,
    g: Option<u8>,
    b: Option<u8>,
    a: Option<u8>,
) -> Result<(), String> {
    use tauri::window::{Color, Effect, EffectsBuilder};

    let apply = |effects: Option<tauri::utils::config::WindowEffectsConfig>| -> Result<(), String> {
        window.set_effects(effects).map_err(|e| e.to_string())
    };

    if !enabled {
        // Best-effort clear; ignore if compositor rejects
        let _ = apply(None);
        return Ok(());
    }

    let color = Color(
        r.unwrap_or(12),
        g.unwrap_or(16),
        b.unwrap_or(26),
        a.unwrap_or(110),
    );

    // Acrylic alone first (Acrylic+Blur together can fault on some drivers).
    let acrylic = EffectsBuilder::new()
        .effects([Effect::Acrylic])
        .color(color)
        .build();
    if apply(Some(acrylic)).is_ok() {
        return Ok(());
    }

    // Fallback: older Blur effect only
    let blur = EffectsBuilder::new()
        .effects([Effect::Blur])
        .color(color)
        .build();
    if apply(Some(blur)).is_ok() {
        return Ok(());
    }

    // Soft-fail: app stays up without glass
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();
    write_crash_log("app start (tauri run)");

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_esword_modules,
            list_bible_books,
            get_bible_chapter,
            get_verse_commentaries,
            search_dictionary,
            search_lexicon,
            search_commentary_term,
            search_bible_word,
            suggest_dictionary_topics,
            resolve_strongs,
            probe_modules_content,
            set_window_glass,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        write_crash_log(&format!("tauri run error: {e}"));
        eprintln!("error while running tauri application: {e}");
        // Don't abort via expect — leave a log breadcrumb first.
        std::process::exit(1);
    }
}
