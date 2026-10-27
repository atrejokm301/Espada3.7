mod books;
mod crossrefs;
mod modules;
mod text;

/// Representative workload for instrumentation PGO / AutoFDO-style training runs.
/// Invoked by `pgo_harness` binary (no GUI).
pub fn pgo_smoke(modules_dir: &str) -> Result<String, String> {
    use modules::*;
    let mods = scan_modules(modules_dir)?;
    let bibles: Vec<_> = mods
        .iter()
        .filter(|m| m.kind == "bible" && !m.encrypted)
        .take(6)
        .collect();
    let dicts: Vec<_> = mods
        .iter()
        .filter(|m| m.kind == "dictionary" && !m.encrypted)
        .take(4)
        .map(|m| m.path.clone())
        .collect();
    let lexs: Vec<_> = mods
        .iter()
        .filter(|m| m.kind == "lexicon" && !m.encrypted)
        .take(4)
        .map(|m| m.path.clone())
        .collect();
    let cmts: Vec<_> = mods
        .iter()
        .filter(|m| m.kind == "commentary" && !m.encrypted)
        .take(4)
        .collect();

    let mut n_ops = 0u64;
    for b in &bibles {
        for (book, ch) in [(43i32, 3i32), (1, 1), (40, 5), (24, 1)] {
            let _ = load_bible_chapter(&b.path, book, ch);
            let _ = load_verse_text(&b.path, book, ch, 1);
            n_ops += 2;
        }
    }
    for c in &cmts {
        let _ = load_commentary(&c.path, 24, 1, 1, true, true, false);
        let _ = load_commentary(&c.path, 43, 3, 16, true, false, false);
        n_ops += 2;
    }
    for d in &dicts {
        let _ = dictionary_lookup(d, "amor", 10);
        let _ = dictionary_lookup(d, "Dios", 10);
        let _ = dictionary_lookup(d, "Jeremías", 8);
        n_ops += 3;
    }
    for l in &lexs {
        let _ = lexicon_lookup(l, "G26");
        let _ = lexicon_lookup(l, "H430");
        let _ = lexicon_lookup(l, "H3414");
        n_ops += 3;
    }
    let inter = find_interlinear_path(&mods);
    let _ = resolve_word_links(
        "Jeremías",
        &dicts,
        &lexs,
        inter.as_deref(),
        24,
        1,
        1,
    );
    let _ = resolve_word_links("amor", &dicts, &lexs, inter.as_deref(), 43, 3, 16);
    let _ = text::clean_esword_text("<ref>Gén 1:1-2</ref> y <num>G26</num> <red>texto</red>");
    let _ = books::resolve_book_name("Pro");
    let _ = books::resolve_book_name("Jer");
    n_ops += 10;
    clear_caches();
    Ok(format!(
        "pgo_smoke ok modules={} bibles={} ops={}",
        mods.len(),
        bibles.len(),
        n_ops
    ))
}

use modules::{
    clear_caches, dictionary_lookup, find_default_modules_dir, find_interlinear_path,
    load_bible_chapter, load_commentary, load_verse_text, lexicon_lookup, probe_commentary_has,
    probe_dictionary_has, probe_lexicon_has, reference_lookup, resolve_word_links, scan_modules,
    verse_count, ChapterPayload, DictEntry, ModuleInfo, StudyEntry, WordLinkResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

struct AppState {
    chapter_cache: Mutex<HashMap<String, ChapterPayload>>,
}

fn chapter_key(path: &str, book: i32, chapter: i32) -> String {
    format!("{path}|{book}|{chapter}")
}

/// Enable/disable true macOS frosted glass (desktop/wallpaper/apps bleed behind the window).
///
/// Critical pieces (all required for real bleed-through):
/// 1. NSWindow opaque=false + clear background
/// 2. WKWebView drawsBackground=false (via set_background_color alpha 0)
/// 3. NSVisualEffectView with BehindWindow blending (window-vibrancy)
/// 4. Transparent CSS in the frosted theme (no solid body fill)
///
/// Note: Tauri's `set_effects(None)` does **not** clear vibrancy on macOS — we clear
/// with `window-vibrancy::clear_vibrancy`.
#[tauri::command]
fn set_frosted_window(window: tauri::WebviewWindow, enabled: bool) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use std::sync::mpsc;
        use tauri::window::{Color, Effect, EffectState, EffectsBuilder};
        use window_vibrancy::{
            apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
        };

        fn log_frost(msg: &str) {
            let line = format!(
                "[{}] {}\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                msg
            );
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/espada-frost.log")
                .and_then(|mut f| {
                    use std::io::Write;
                    f.write_all(line.as_bytes())
                });
            eprintln!("[Espada frost] {msg}");
        }

        /// Tag used by window-vibrancy for its NSVisualEffectView (see their macos/internal.rs).
        const BLUR_VIEW_TAG: isize = 91376254;

        /// Make the AppKit NSWindow fully non-opaque so vibrancy can sample desktop.
        /// When `force_dark`, lock window + vibrancy view to Dark Aqua so overnight
        /// system light mode (or sleep) does not turn HudWindow frost milky-white.
        unsafe fn set_nswindow_clear(
            window: &tauri::WebviewWindow,
            clear: bool,
            force_dark: bool,
        ) -> Result<(), String> {
            use objc2_app_kit::{
                NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSColor,
                NSView, NSWindow,
            };
            use objc2_foundation::MainThreadMarker;

            let _mtm = MainThreadMarker::new().ok_or_else(|| {
                "set_nswindow_clear must run on the AppKit main thread".to_string()
            })?;

            let handle = window
                .window_handle()
                .map_err(|e| format!("window_handle: {e}"))?;
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return Err("not an AppKit window".into());
            };

            // ns_view is the content view (or webview host); climb to NSWindow.
            let ns_view = appkit.ns_view.as_ptr() as *const NSView;
            if ns_view.is_null() {
                return Err("null ns_view".into());
            }
            let view: &NSView = &*ns_view;
            let Some(ns_window) = view.window() else {
                return Err("NSView has no NSWindow".into());
            };
            let ns_window: &NSWindow = &ns_window;

            // Always keep the window movable (drag was stuck after frost/transparency work).
            ns_window.setMovable(true);

            if clear {
                ns_window.setOpaque(false);
                // clearColor = fully transparent window chrome under the webview
                ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
                // Allow the window to show desktop behind titlebar/content
                ns_window.setHasShadow(true);
                // Drag from frosted empty areas (not only the titlebar strip)
                ns_window.setMovableByWindowBackground(true);

                if force_dark {
                    // Pin to dark appearance so NSVisualEffectView stays navy/dark,
                    // not light/white after sleep or system appearance flips.
                    if let Some(dark) = NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua) {
                        ns_window.setAppearance(Some(&dark));
                        view.setAppearance(Some(&dark));
                        // window-vibrancy inserts a tagged NSVisualEffectView as a sibling
                        if let Some(blur) = view.viewWithTag(BLUR_VIEW_TAG) {
                            blur.setAppearance(Some(&dark));
                        }
                        if let Some(content) = ns_window.contentView() {
                            content.setAppearance(Some(&dark));
                            if let Some(blur) = content.viewWithTag(BLUR_VIEW_TAG) {
                                blur.setAppearance(Some(&dark));
                            }
                        }
                    }
                }
            } else {
                ns_window.setOpaque(true);
                ns_window.setBackgroundColor(Some(&NSColor::blackColor()));
                // Solid themes: move via titlebar drag-region only
                ns_window.setMovableByWindowBackground(false);
                // Release forced dark so other themes follow system if desired
                ns_window.setAppearance(None);
                view.setAppearance(None);
            }
            Ok(())
        }

        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        let win = window.clone();
        window
            .run_on_main_thread(move || {
                let result = (|| -> Result<String, String> {
                    // Always clear stacked vibrancy first.
                    match clear_vibrancy(&win) {
                        Ok(true) => log_frost("cleared previous vibrancy view"),
                        Ok(false) => log_frost("no previous vibrancy view"),
                        Err(e) => log_frost(&format!("clear_vibrancy warn: {e}")),
                    }
                    // Also clear Tauri effects state if any
                    let _ = win.set_effects(None::<tauri::utils::config::WindowEffectsConfig>);

                    if enabled {
                        // Transparent webview first (private drawsBackground=false via wry)
                        win.set_background_color(Some(Color(0, 0, 0, 0)))
                            .map_err(|e| format!("set_background_color: {e}"))?;

                        // Prefer UnderWindowBackground (desktop sample) + force dark appearance.
                        // HudWindow alone follows system light mode → milky white overnight.
                        apply_vibrancy(
                            &win,
                            NSVisualEffectMaterial::UnderWindowBackground,
                            Some(NSVisualEffectState::Active),
                            Some(20.0),
                        )
                        .map_err(|e| format!("apply_vibrancy(UnderWindowBackground): {e}"))?;

                        // After the blur view exists, clear window + force Dark Aqua on it
                        unsafe {
                            set_nswindow_clear(&win, true, true)?;
                        }

                        let _ = win.set_effects(
                            EffectsBuilder::new()
                                .effect(Effect::UnderWindowBackground)
                                .state(EffectState::Active)
                                .radius(20.0)
                                .build(),
                        );

                        // Re-assert dark appearance once more (set_effects may recreate material)
                        unsafe {
                            set_nswindow_clear(&win, true, true)?;
                        }

                        log_frost("enabled UnderWindowBackground + DarkAqua lock");
                        Ok("frost-on:under-dark".into())
                    } else {
                        unsafe {
                            set_nswindow_clear(&win, false, false)?;
                        }
                        // Solid black under non-frost themes (CSS paints the real theme color)
                        win.set_background_color(Some(Color(0, 0, 0, 255)))
                            .map_err(|e| format!("set_background_color solid: {e}"))?;
                        log_frost("disabled vibrancy; opaque window");
                        Ok("frost-off".into())
                    }
                })();
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;

        // Wait for main-thread work so JS knows if frost really applied
        match rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(r) => r,
            Err(e) => Err(format!("frost main-thread timeout/disconnect: {e}")),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, enabled);
        Ok("frost-noop-non-macos".into())
    }
}

#[tauri::command]
fn get_books() -> serde_json::Value {
    books::books_json()
}

#[tauri::command]
fn get_default_modules_dir() -> Option<String> {
    find_default_modules_dir()
}

#[tauri::command]
fn list_modules(modules_dir: String) -> Result<Vec<ModuleInfo>, String> {
    scan_modules(&modules_dir)
}

#[tauri::command]
fn refresh_modules(modules_dir: String) -> Result<Vec<ModuleInfo>, String> {
    clear_caches();
    scan_modules(&modules_dir)
}

#[tauri::command]
fn get_bible_chapter(
    state: tauri::State<'_, AppState>,
    module_path: String,
    book: i32,
    chapter: i32,
) -> Result<ChapterPayload, String> {
    let key = chapter_key(&module_path, book, chapter);
    {
        let cache = state.chapter_cache.lock().unwrap();
        if let Some(c) = cache.get(&key) {
            return Ok(c.clone());
        }
    }
    let payload = load_bible_chapter(&module_path, book, chapter)?;
    {
        let mut cache = state.chapter_cache.lock().unwrap();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, payload.clone());
    }
    Ok(payload)
}

#[tauri::command]
fn get_verse_plain(
    module_path: String,
    book: i32,
    chapter: i32,
    verse: i32,
) -> Result<String, String> {
    load_verse_text(&module_path, book, chapter, verse)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PassageVerse {
    verse: i32,
    plain: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PassagePayload {
    book: i32,
    chapter: i32,
    verse_start: i32,
    verse_end: i32,
    abbreviation: String,
    title: String,
    verses: Vec<PassageVerse>,
}

/// Load one or more verses from the currently chosen Bible module (for hover/passage preview).
#[tauri::command]
fn get_passage(
    state: tauri::State<'_, AppState>,
    module_path: String,
    book: i32,
    chapter: i32,
    verse_start: i32,
    verse_end: Option<i32>,
) -> Result<PassagePayload, String> {
    let mut v0 = verse_start.max(1);
    let mut v1 = verse_end.unwrap_or(v0).max(1);
    if v1 < v0 {
        std::mem::swap(&mut v0, &mut v1);
    }
    // Cap range so hover stays responsive
    if v1 - v0 > 40 {
        v1 = v0 + 40;
    }

    // Reuse chapter cache (same as main Bible reader)
    let key = chapter_key(&module_path, book, chapter);
    let chapter_data = {
        let cache = state.chapter_cache.lock().unwrap();
        cache.get(&key).cloned()
    };
    let chapter_data = if let Some(c) = chapter_data {
        c
    } else {
        let payload = load_bible_chapter(&module_path, book, chapter)?;
        let mut cache = state.chapter_cache.lock().unwrap();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, payload.clone());
        payload
    };

    let abbr = chapter_data.abbreviation.clone();
    let title = chapter_data.title.clone();
    let verses: Vec<PassageVerse> = chapter_data
        .verses
        .into_iter()
        .filter(|v| v.verse >= v0 && v.verse <= v1)
        .map(|v| PassageVerse {
            verse: v.verse,
            plain: v.plain,
        })
        .collect();

    if verses.is_empty() {
        let plain = load_verse_text(&module_path, book, chapter, v0)?;
        return Ok(PassagePayload {
            book,
            chapter,
            verse_start: v0,
            verse_end: v0,
            abbreviation: abbr,
            title,
            verses: vec![PassageVerse { verse: v0, plain }],
        });
    }

    Ok(PassagePayload {
        book,
        chapter,
        verse_start: verses.first().map(|v| v.verse).unwrap_or(v0),
        verse_end: verses.last().map(|v| v.verse).unwrap_or(v1),
        abbreviation: abbr,
        title,
        verses,
    })
}

#[tauri::command]
fn get_verse_count(module_path: String, book: i32, chapter: i32) -> Result<i32, String> {
    verse_count(&module_path, book, chapter)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentaryFilters {
    verse: bool,
    chapter: bool,
    book: bool,
}

#[tauri::command]
fn get_commentary(
    module_path: String,
    book: i32,
    chapter: i32,
    verse: i32,
    filters: CommentaryFilters,
) -> Result<Vec<StudyEntry>, String> {
    load_commentary(
        &module_path,
        book,
        chapter,
        verse,
        filters.verse,
        filters.chapter,
        filters.book,
    )
}

#[tauri::command]
fn lookup_dictionary(
    module_path: String,
    query: String,
    limit: Option<i32>,
) -> Result<Vec<DictEntry>, String> {
    dictionary_lookup(&module_path, &query, limit.unwrap_or(30))
}

#[tauri::command]
fn lookup_lexicon(module_path: String, strong: String) -> Result<Option<DictEntry>, String> {
    lexicon_lookup(&module_path, &strong)
}

#[tauri::command]
fn lookup_lexicon_multi(
    module_paths: Vec<String>,
    strong: String,
) -> Result<Option<DictEntry>, String> {
    let mut last_err: Option<String> = None;
    for p in module_paths {
        match lexicon_lookup(&p, &strong) {
            Ok(Some(e)) => return Ok(Some(e)),
            Ok(None) => {}
            Err(e) => {
                // Skip encrypted / bad modules; keep searching
                last_err = Some(e);
            }
        }
    }
    if last_err.is_some() && strong.trim().is_empty() {
        return Err(last_err.unwrap());
    }
    Ok(None)
}

#[tauri::command]
fn lookup_reference(
    module_path: String,
    query: String,
    limit: Option<i32>,
) -> Result<Vec<DictEntry>, String> {
    reference_lookup(&module_path, &query, limit.unwrap_or(40))
}

#[tauri::command]
fn resolve_word(
    word: String,
    dict_paths: Vec<String>,
    lex_paths: Vec<String>,
    interlinear_path: Option<String>,
    book: i32,
    chapter: i32,
    verse: i32,
) -> WordLinkResult {
    resolve_word_links(
        &word,
        &dict_paths,
        &lex_paths,
        interlinear_path.as_deref(),
        book,
        chapter,
        verse,
    )
}

#[tauri::command]
fn find_interlinear(modules_dir: String) -> Option<String> {
    let mods = scan_modules(&modules_dir).ok()?;
    find_interlinear_path(&mods)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResult {
    path: String,
    has: bool,
}

#[tauri::command]
fn probe_modules_content(
    kind: String,
    paths: Vec<String>,
    book: i32,
    chapter: i32,
    verse: i32,
    query: String,
    filters: Option<CommentaryFilters>,
) -> Vec<ProbeResult> {
    let f = filters.unwrap_or(CommentaryFilters {
        verse: true,
        chapter: false,
        book: false,
    });
    paths
        .into_iter()
        .map(|path| {
            let has = match kind.as_str() {
                "commentary" => {
                    probe_commentary_has(&path, book, chapter, verse, f.verse, f.chapter, f.book)
                }
                "dictionary" => probe_dictionary_has(&path, &query),
                "lexicon" => {
                    if query.trim().is_empty() {
                        lexicon_lookup(&path, "G26")
                            .map(|v| v.is_some())
                            .unwrap_or(false)
                            || lexicon_lookup(&path, "H430")
                                .map(|v| v.is_some())
                                .unwrap_or(false)
                    } else {
                        probe_lexicon_has(&path, &query)
                    }
                }
                "reference" => reference_lookup(&path, &query, 1)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                _ => false,
            };
            ProbeResult { path, has }
        })
        .collect()
}

#[tauri::command]
fn resolve_book_ref(raw: String) -> Option<i32> {
    books::resolve_book_name(&raw)
}

#[tauri::command]
fn list_cross_reference_sources(
    modules_dir: String,
) -> Result<Vec<crossrefs::CrossRefSource>, String> {
    let mods = scan_modules(&modules_dir)?;
    Ok(crossrefs::list_cross_reference_sources(&mods, &modules_dir))
}

#[tauri::command]
fn get_cross_references(
    modules_dir: String,
    book: i32,
    chapter: i32,
    verse: i32,
    source_path: Option<String>,
    limit: Option<i32>,
) -> Result<crossrefs::CrossRefPayload, String> {
    let mods = scan_modules(&modules_dir)?;
    crossrefs::load_cross_references(
        &mods,
        &modules_dir,
        book,
        chapter,
        verse,
        source_path.as_deref(),
        limit.unwrap_or(80),
    )
}

/// Async folder picker — avoids blocking the main thread (macOS crash with blocking_pick_folder).
#[tauri::command]
async fn pick_modules_dir(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    app.dialog()
        .file()
        .set_title("Seleccionar carpeta de módulos e-Sword")
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });

    // Wait off the async executor so the dialog event loop can run
    let folder = tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| format!("Error del diálogo: {e}"))?
        .map_err(|e| format!("Diálogo cancelado o falló: {e}"))?;

    Ok(folder.map(|p| p.to_string()))
}

/// Set modules path by string (fallback if the native dialog fails).
#[tauri::command]
fn set_modules_path(path: String) -> Result<String, String> {
    let p = std::path::Path::new(path.trim());
    if !p.is_dir() {
        return Err(format!("No es una carpeta válida: {path}"));
    }
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
fn clean_text(raw: String) -> String {
    text::clean_esword_text(&raw)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            chapter_cache: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            set_frosted_window,
            get_books,
            get_default_modules_dir,
            list_modules,
            refresh_modules,
            get_bible_chapter,
            get_verse_plain,
            get_passage,
            get_verse_count,
            get_commentary,
            lookup_dictionary,
            lookup_lexicon,
            lookup_lexicon_multi,
            lookup_reference,
            resolve_word,
            find_interlinear,
            probe_modules_content,
            resolve_book_ref,
            list_cross_reference_sources,
            get_cross_references,
            pick_modules_dir,
            set_modules_path,
            clean_text,
        ])
        .setup(|app| {
            // Start transparent so Frosted Glass can bleed desktop. Solid themes
            // paint CSS fills and call set_frosted_window(false) to go opaque.
            if let Some(w) = app.get_webview_window("main") {
                use tauri::window::Color;
                let _ = w.set_background_color(Some(Color(0, 0, 0, 0)));
                #[cfg(target_os = "macos")]
                {
                    // Best-effort: clear NSWindow early (may no-op if view not ready)
                    let win = w.clone();
                    let _ = w.run_on_main_thread(move || {
                        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                        use objc2_app_kit::{NSColor, NSView, NSWindow};
                        if let Ok(handle) = win.window_handle() {
                            if let RawWindowHandle::AppKit(a) = handle.as_raw() {
                                unsafe {
                                    let ns_view = a.ns_view.as_ptr() as *const NSView;
                                    if !ns_view.is_null() {
                                        let view: &NSView = &*ns_view;
                                        if let Some(nsw) = view.window() {
                                            let nsw: &NSWindow = &nsw;
                                            nsw.setOpaque(false);
                                            nsw.setBackgroundColor(Some(&NSColor::clearColor()));
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Espada3.7");
}
