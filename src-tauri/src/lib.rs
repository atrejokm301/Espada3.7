mod bible_titles;
mod books;
mod conjugation;
mod crossrefs;
mod modules;
mod sections;
mod strong_gloss;
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
    probe_dictionary_has, probe_lexicon_has, reference_lookup, resolve_word_links_at,
    scan_modules, verse_count, ChapterPayload, DictEntry, ModuleInfo,
    ResolveOpts, StudyEntry, WordLinkResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use tauri::Manager;

/// Bible chapters already rendered, keyed by [`chapter_key`]. `generation` is bumped by every
/// clear (ISS-46), so a load that started before «Actualizar lista» or a folder switch can
/// never put the old text back after the clear.
#[derive(Default)]
struct ChapterCache {
    generation: u64,
    map: HashMap<String, ChapterPayload>,
}

const CHAPTER_CACHE_CAP: usize = 64;

#[derive(Default)]
struct AppState {
    chapter_cache: Mutex<ChapterCache>,
    /// Last folder passed to list_modules / refresh_modules (ISS-46 folder-switch detection).
    modules_dir: Mutex<Option<String>>,
}

/// A panic elsewhere must not turn every later lookup into a poisoned-lock panic.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    /// Cached chapter (if any) plus the generation to hand back to [`AppState::chapter_put`].
    fn chapter_get(&self, key: &str) -> (Option<ChapterPayload>, u64) {
        let c = lock(&self.chapter_cache);
        (c.map.get(key).cloned(), c.generation)
    }

    /// Insert unless the cache was cleared after `generation` was read (ISS-46 race).
    fn chapter_put(&self, key: String, payload: ChapterPayload, generation: u64) {
        let mut c = lock(&self.chapter_cache);
        if c.generation != generation {
            return;
        }
        if c.map.len() > CHAPTER_CACHE_CAP {
            c.map.clear();
        }
        c.map.insert(key, payload);
    }

    /// ISS-46: drop every cached chapter and invalidate loads still in flight.
    fn clear_chapter_cache(&self) {
        let mut c = lock(&self.chapter_cache);
        c.generation = c.generation.wrapping_add(1);
        c.map.clear();
    }

    /// Remember `dir`; true when it replaces a different, previously used folder.
    fn note_modules_dir(&self, dir: &str) -> bool {
        let mut last = lock(&self.modules_dir);
        let changed = matches!(last.as_deref(), Some(prev) if prev != dir);
        *last = Some(dir.to_string());
        changed
    }

    /// list_modules bookkeeping: on a folder switch the chapter cache is cleared here and
    /// the caller must clear the module caches (returns true).
    fn on_list_modules(&self, dir: &str) -> bool {
        let switched = self.note_modules_dir(dir);
        if switched {
            self.clear_chapter_cache();
        }
        switched
    }

    /// refresh_modules bookkeeping: always clears the chapter cache.
    fn on_refresh_modules(&self, dir: &str) {
        self.note_modules_dir(dir);
        self.clear_chapter_cache();
    }
}

/// Blocking part of list_modules / refresh_modules: optionally drop every module cache
/// (connections, schemas, module list, tag/remap/lemma caches), then scan.
fn scan_modules_after(clear: bool, modules_dir: &str) -> Result<Vec<ModuleInfo>, String> {
    if clear {
        clear_caches();
    }
    scan_modules(modules_dir)
}

fn chapter_key(path: &str, book: i32, chapter: i32) -> String {
    format!("{path}|{book}|{chapter}")
}

/// At most this many module jobs run at once. The main thread used to serialize every
/// command; now the side pane's `Promise.all` (one get_commentary per commentary, ~100)
/// would open that many SQLite files together. The cap keeps fds near ISS-28's bound
/// (64 cached connections + this many in flight) under the GUI's 256 soft limit.
const MAX_PARALLEL_DB_JOBS: usize = 8;

static DB_JOBS: (Mutex<usize>, std::sync::Condvar) = (Mutex::new(0), std::sync::Condvar::new());

/// One of [`MAX_PARALLEL_DB_JOBS`] slots; released on drop (also when the job panics).
struct DbPermit;

impl DbPermit {
    fn acquire() -> Self {
        let (count, cv) = &DB_JOBS;
        let mut n = lock(count);
        while *n >= MAX_PARALLEL_DB_JOBS {
            n = cv.wait(n).unwrap_or_else(|e| e.into_inner());
        }
        *n += 1;
        DbPermit
    }
}

impl Drop for DbPermit {
    fn drop(&mut self) {
        let (count, cv) = &DB_JOBS;
        let mut n = lock(count);
        *n = n.saturating_sub(1);
        cv.notify_one();
    }
}

/// ISS-11: run blocking SQLite / filesystem work on Tauri's blocking pool, never on the
/// AppKit main thread (sync commands run inline there). No lock is held across the await;
/// the job holds one [`DbPermit`] while it runs and never waits for a second one.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = DbPermit::acquire();
        f()
    })
    .await
    .map_err(|e| format!("Error interno al consultar el módulo: {e}"))
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
async fn list_modules(
    state: tauri::State<'_, AppState>,
    modules_dir: String,
) -> Result<Vec<ModuleInfo>, String> {
    // ISS-46: a different folder than last time drops the old folder's chapters and caches.
    let switched = state.on_list_modules(&modules_dir);
    blocking(move || scan_modules_after(switched, &modules_dir)).await?
}

#[tauri::command]
async fn refresh_modules(
    state: tauri::State<'_, AppState>,
    modules_dir: String,
) -> Result<Vec<ModuleInfo>, String> {
    // ISS-46: «Actualizar lista» also forgets cached Bible chapters (State is injected; the UI
    // still sends only modulesDir).
    state.on_refresh_modules(&modules_dir);
    blocking(move || scan_modules_after(true, &modules_dir)).await?
}

/// Chapter from the shared cache, else loaded off the main thread (ISS-11) and cached unless a
/// clear happened meanwhile (ISS-46).
async fn cached_chapter(
    state: &AppState,
    module_path: &str,
    book: i32,
    chapter: i32,
) -> Result<ChapterPayload, String> {
    let key = chapter_key(module_path, book, chapter);
    let (hit, generation) = state.chapter_get(&key);
    if let Some(c) = hit {
        return Ok(c);
    }
    let path = module_path.to_string();
    let payload = blocking(move || load_bible_chapter(&path, book, chapter)).await??;
    state.chapter_put(key, payload.clone(), generation);
    Ok(payload)
}

#[tauri::command]
async fn get_bible_chapter(
    state: tauri::State<'_, AppState>,
    module_path: String,
    book: i32,
    chapter: i32,
) -> Result<ChapterPayload, String> {
    cached_chapter(state.inner(), &module_path, book, chapter).await
}

#[tauri::command]
async fn get_verse_plain(
    module_path: String,
    book: i32,
    chapter: i32,
    verse: i32,
) -> Result<String, String> {
    blocking(move || load_verse_text(&module_path, book, chapter, verse)).await?
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

/// Normalized passage range: both ends ≥ 1, reversed ranges swapped, capped at 41 verses
/// so hover stays responsive.
fn passage_range(verse_start: i32, verse_end: Option<i32>) -> (i32, i32) {
    let mut v0 = verse_start.max(1);
    let mut v1 = verse_end.unwrap_or(v0).max(1);
    if v1 < v0 {
        std::mem::swap(&mut v0, &mut v1);
    }
    if v1 - v0 > 40 {
        v1 = v0 + 40;
    }
    (v0, v1)
}

/// The verses of `chapter_data` inside v0..=v1; `Err((abbreviation, title))` when none fall in
/// the range (the caller then loads the single verse v0, as before).
fn passage_from_chapter(
    chapter_data: ChapterPayload,
    book: i32,
    chapter: i32,
    v0: i32,
    v1: i32,
) -> Result<PassagePayload, (String, String)> {
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
        return Err((abbr, title));
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

/// Load one or more verses from the currently chosen Bible module (for hover/passage preview).
#[tauri::command]
async fn get_passage(
    state: tauri::State<'_, AppState>,
    module_path: String,
    book: i32,
    chapter: i32,
    verse_start: i32,
    verse_end: Option<i32>,
) -> Result<PassagePayload, String> {
    let (v0, v1) = passage_range(verse_start, verse_end);
    // Reuse chapter cache (same as main Bible reader)
    let chapter_data = cached_chapter(state.inner(), &module_path, book, chapter).await?;
    match passage_from_chapter(chapter_data, book, chapter, v0, v1) {
        Ok(p) => Ok(p),
        Err((abbreviation, title)) => {
            let plain =
                blocking(move || load_verse_text(&module_path, book, chapter, v0)).await??;
            Ok(PassagePayload {
                book,
                chapter,
                verse_start: v0,
                verse_end: v0,
                abbreviation,
                title,
                verses: vec![PassageVerse { verse: v0, plain }],
            })
        }
    }
}

#[tauri::command]
async fn get_verse_count(module_path: String, book: i32, chapter: i32) -> Result<i32, String> {
    blocking(move || verse_count(&module_path, book, chapter)).await?
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentaryFilters {
    verse: bool,
    chapter: bool,
    book: bool,
}

#[tauri::command]
async fn get_commentary(
    module_path: String,
    book: i32,
    chapter: i32,
    verse: i32,
    filters: CommentaryFilters,
    sections: Option<bool>,
) -> Result<Vec<StudyEntry>, String> {
    blocking(move || {
        // `sections`: the main panel — the module's section titles (Catena Aurea's Fathers…).
        modules::with_section_titles(&module_path, sections.unwrap_or(false), || {
            load_commentary(&module_path, book, chapter, verse, filters.verse, filters.chapter, filters.book)
        })
    })
    .await?
}

/// `strongs` + `lex_paths` (optional, sent for a resolved tap): when the query has no full
/// match in this module, the Strong bridge's headwords come first (amó → AMOR, `strong`).
#[tauri::command]
async fn lookup_dictionary(
    module_path: String,
    query: String,
    limit: Option<i32>,
    strongs: Option<Vec<String>>,
    lex_paths: Option<Vec<String>>,
    sections: Option<bool>,
) -> Result<Vec<DictEntry>, String> {
    blocking(move || {
        let strongs: Vec<String> = strongs.unwrap_or_default().into_iter().take(2).collect();
        let lex_paths = lex_paths.unwrap_or_default();
        modules::with_section_titles(&module_path, sections.unwrap_or(false), || {
            if strongs.is_empty() || lex_paths.is_empty() {
                dictionary_lookup(&module_path, &query, limit.unwrap_or(30))
            } else {
                modules::dictionary_lookup_bridged(&module_path, &query, limit.unwrap_or(30), &strongs, &lex_paths)
            }
        })
    })
    .await?
}

/// e-Sword style topic index of a dictionary: a window of its alphabetical topic list around
/// `query` (or `anchor`), with where the query is (`focus`) and whether it is a topic (`exact`).
/// `lexicon` (the Léxico tab): the `Lexicon` table first, Strong's rows with their headword
/// and the Griego/Hebreo starts. Absent = the Diccionario's index, as before.
#[tauri::command]
async fn dictionary_index(
    module_path: String,
    query: Option<String>,
    anchor: Option<u32>,
    before: Option<u32>,
    after: Option<u32>,
    lexicon: Option<bool>,
) -> Result<modules::IndexSlice, String> {
    blocking(move || {
        modules::topic_index(
            &module_path,
            query.as_deref(),
            anchor,
            before.unwrap_or(40),
            after.unwrap_or(80),
            lexicon.unwrap_or(false),
        )
    })
    .await?
}

/// Devocional: the (month, day) pairs a devotional (.devi) has a reading for.
#[tauri::command]
async fn list_devotional_days(module_path: String) -> Result<Vec<(i32, i32)>, String> {
    blocking(move || modules::devotional_days(&module_path)).await?
}

/// Devocional: one day's reading (`None` when the devotional has none for that day).
#[tauri::command]
async fn get_devotional(module_path: String, month: i32, day: i32) -> Result<Option<modules::DevotionalEntry>, String> {
    blocking(move || modules::devotional_entry(&module_path, month, day)).await?
}

/// «ES» line of the Léxico: the Reina-Valera renderings a Spanish Strong's dictionary gives for
/// `code` («G26» → «ágape, amado, amor»), from the first of `module_paths` (at most 4: the page
/// sends the Spanish Strong's dictionaries, its preferred one first) that has them.
#[tauri::command]
async fn strong_gloss(code: String, module_paths: Vec<String>) -> Result<Option<strong_gloss::StrongGloss>, String> {
    blocking(move || modules::strong_gloss(&code, &module_paths)).await?
}

/// The article of one index row (exactly that topic), from the table its index read.
#[tauri::command]
async fn get_dictionary_entry(
    module_path: String,
    rowid: i64,
    lexicon: Option<bool>,
    sections: Option<bool>,
) -> Result<Option<DictEntry>, String> {
    blocking(move || {
        modules::with_section_titles(&module_path, sections.unwrap_or(false), || {
            modules::topic_entry_by_rowid(&module_path, rowid, lexicon.unwrap_or(false))
        })
    })
    .await?
}

#[tauri::command]
async fn lookup_lexicon(module_path: String, strong: String, sections: Option<bool>) -> Result<Option<DictEntry>, String> {
    blocking(move || {
        modules::with_section_titles(&module_path, sections.unwrap_or(false), || lexicon_lookup(&module_path, &strong))
    })
    .await?
}

fn lexicon_multi(module_paths: Vec<String>, strong: &str) -> Result<Option<DictEntry>, String> {
    let mut last_err: Option<String> = None;
    for p in module_paths {
        match lexicon_lookup(&p, strong) {
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
async fn lookup_lexicon_multi(
    module_paths: Vec<String>,
    strong: String,
) -> Result<Option<DictEntry>, String> {
    blocking(move || lexicon_multi(module_paths, &strong)).await?
}

#[tauri::command]
async fn lookup_reference(
    module_path: String,
    query: String,
    limit: Option<i32>,
) -> Result<Vec<DictEntry>, String> {
    blocking(move || reference_lookup(&module_path, &query, limit.unwrap_or(40))).await?
}

/// Upper bound on `taggedStrongs` from the webview (a tagged word carries 1–3 codes).
const MAX_TAGGED_STRONGS: usize = 16;

/// ISS-35: the reading Bible only when it is still a file. The UI re-sends a stored biblePath
/// on relaunch without checking, so a removed or renamed module simply means «no remap»
/// (identity numbering), never an error.
fn usable_bible_path(bible_path: Option<String>) -> Option<String> {
    let p = bible_path?;
    if p.trim().is_empty() || !std::path::Path::new(&p).is_file() {
        return None;
    }
    Some(p)
}

/// ISS-02/35/36: optional resolve_word arguments → [`ResolveOpts`]. All `None` gives
/// `ResolveOpts::default()`, i.e. exactly the old `resolve_word_links` call.
fn resolve_opts(
    typed: Option<bool>,
    bible_path: Option<String>,
    tagged_strongs: Option<Vec<String>>,
) -> ResolveOpts {
    ResolveOpts {
        typed: typed.unwrap_or(false),
        tagged_strongs: tagged_strongs
            .unwrap_or_default()
            .into_iter()
            .take(MAX_TAGGED_STRONGS)
            .collect(),
        bible_path: usable_bible_path(bible_path),
        ..ResolveOpts::default()
    }
}

/// ISS-28: the connections every tap needs (interlinear, reading Bible) are kept out of LRU
/// eviction. `None` when there is no interlinear (nothing worth pinning).
fn resolve_pins(interlinear_path: Option<&str>, bible_path: Option<&str>) -> Option<Vec<String>> {
    let ip = interlinear_path.filter(|p| !p.is_empty())?;
    let mut pins = vec![ip.to_string()];
    pins.extend(bible_path.filter(|p| !p.is_empty() && *p != ip).map(str::to_string));
    Some(pins)
}

fn pin_resolve_modules(interlinear_path: Option<&str>, bible_path: Option<&str>) {
    if let Some(mut pins) = resolve_pins(interlinear_path, bible_path) {
        // The reading Bible's Strong-tagged twin answers its untagged taps: keep it open too.
        if let Some(twin) = bible_path.filter(|p| !p.is_empty()).and_then(modules::tagged_twin) {
            if !pins.contains(&twin) {
                pins.push(twin);
            }
        }
        // The pin set is process-global; the modules tests assert on it while running in
        // parallel, so unit tests check `resolve_pins` and leave the global set alone.
        #[cfg(not(test))]
        modules::set_pinned_modules(&pins);
        #[cfg(test)]
        let _ = pins;
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn resolve_word(
    word: String,
    dict_paths: Vec<String>,
    lex_paths: Vec<String>,
    interlinear_path: Option<String>,
    book: i32,
    chapter: i32,
    verse: i32,
    occurrence: Option<usize>,
    typed: Option<bool>,
    bible_path: Option<String>,
    tagged_strongs: Option<Vec<String>>,
) -> Result<WordLinkResult, String> {
    blocking(move || {
        let opts = resolve_opts(typed, bible_path, tagged_strongs);
        pin_resolve_modules(interlinear_path.as_deref(), opts.bible_path.as_deref());
        resolve_word_links_at(
            &word,
            &dict_paths,
            &lex_paths,
            interlinear_path.as_deref(),
            book,
            chapter,
            verse,
            occurrence,
            &opts,
        )
    })
    .await
}

#[tauri::command]
async fn find_interlinear(modules_dir: String) -> Option<String> {
    blocking(move || {
        let mods = scan_modules(&modules_dir).ok()?;
        find_interlinear_path(&mods)
    })
    .await
    .ok()
    .flatten()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResult {
    path: String,
    has: bool,
}

fn probe_content(
    kind: &str,
    paths: Vec<String>,
    book: i32,
    chapter: i32,
    verse: i32,
    query: &str,
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
            let has = match kind {
                "commentary" => {
                    probe_commentary_has(&path, book, chapter, verse, f.verse, f.chapter, f.book)
                }
                "dictionary" => probe_dictionary_has(&path, query),
                "lexicon" => {
                    if query.trim().is_empty() {
                        lexicon_lookup(&path, "G26")
                            .map(|v| v.is_some())
                            .unwrap_or(false)
                            || lexicon_lookup(&path, "H430")
                                .map(|v| v.is_some())
                                .unwrap_or(false)
                    } else {
                        probe_lexicon_has(&path, query)
                    }
                }
                "reference" => reference_lookup(&path, query, 1)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                _ => false,
            };
            ProbeResult { path, has }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn probe_modules_content(
    kind: String,
    paths: Vec<String>,
    book: i32,
    chapter: i32,
    verse: i32,
    query: String,
    filters: Option<CommentaryFilters>,
) -> Result<Vec<ProbeResult>, String> {
    blocking(move || probe_content(&kind, paths, book, chapter, verse, &query, filters)).await
}

#[tauri::command]
fn resolve_book_ref(raw: String) -> Option<i32> {
    books::resolve_book_name(&raw)
}

#[tauri::command]
async fn list_cross_reference_sources(
    modules_dir: String,
) -> Result<Vec<crossrefs::CrossRefSource>, String> {
    blocking(move || {
        let mods = scan_modules(&modules_dir)?;
        Ok(crossrefs::list_cross_reference_sources(&mods, &modules_dir))
    })
    .await?
}

#[tauri::command]
async fn get_cross_references(
    modules_dir: String,
    book: i32,
    chapter: i32,
    verse: i32,
    source_path: Option<String>,
    limit: Option<i32>,
) -> Result<crossrefs::CrossRefPayload, String> {
    blocking(move || {
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
    })
    .await?
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

/// ISS-34: may the webview navigate to `url`? Only the app's own origin: `tauri://localhost`
/// (macOS/Linux) or `http(s)://tauri.localhost` (Windows), plus — in debug builds only —
/// the `devUrl` server on port 1420. Module HTML links (http, file, data, javascript, about…)
/// can never take the window away from the app.
fn is_app_navigation(url: &tauri::Url, allow_dev_server: bool) -> bool {
    let host = url.host_str().unwrap_or("");
    match url.scheme() {
        "tauri" => host == "localhost",
        "http" | "https" if host == "tauri.localhost" => url.port().is_none(),
        "http" if allow_dev_server => {
            matches!(host, "127.0.0.1" | "localhost") && url.port() == Some(1420)
        }
        _ => false,
    }
}

/// Navigation guard for every webview (ISS-34). Blocked attempts are logged, never opened.
fn nav_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("espada-nav-guard")
        .on_navigation(|_webview, url| {
            let ok = is_app_navigation(url, cfg!(debug_assertions));
            if !ok {
                eprintln!("[Espada] navegación bloqueada: {url}");
            }
            ok
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(nav_guard())
        .manage(AppState::default())
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
            dictionary_index,
            get_dictionary_entry,
            strong_gloss,
            list_devotional_days,
            get_devotional,
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

/// WP5 regression tests for the command layer (ISS-02/35/36 args, ISS-11 async, ISS-46 cache
/// clearing, ISS-34 navigation guard). Golden tests open the module library READ-ONLY
/// (`ESPADA_MODULES_DIR`, else `$HOME/Downloads/for mac espada`) and print SKIP when it is
/// absent. Synthetic SQLite files go to the system temp dir. These tests never call the
/// process-global `clear_caches()` / `set_pinned_modules()`, which the modules tests assert on.
#[cfg(test)]
mod wp5_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn library() -> Option<PathBuf> {
        let dir = match std::env::var("ESPADA_MODULES_DIR").ok().filter(|s| !s.trim().is_empty()) {
            Some(d) => PathBuf::from(d),
            None => PathBuf::from(std::env::var("HOME").ok()?)
                .join("Downloads")
                .join("for mac espada"),
        };
        if dir.is_dir() {
            Some(dir)
        } else {
            eprintln!("SKIP: library not found: {}", dir.display());
            None
        }
    }

    fn lib_files<const N: usize>(names: [&str; N]) -> Option<[String; N]> {
        let lib = library()?;
        let mut out: [String; N] = std::array::from_fn(|_| String::new());
        for (i, n) in names.iter().enumerate() {
            let p = lib.join(n);
            if !p.is_file() {
                eprintln!("SKIP: module not in library: {n}");
                return None;
            }
            out[i] = p.to_string_lossy().to_string();
        }
        Some(out)
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "espada-wp5-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_bible(path: &std::path::Path, rows: &[(i32, i32, i32, &str)]) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT, Strongs BOOL);
             INSERT INTO Details VALUES ('Prueba','PRB',1,0);
             CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture TEXT);
             CREATE INDEX BookChapterVerseIndex ON Bible (Book, Chapter, Verse);",
        )
        .unwrap();
        for (b, ch, v, t) in rows {
            c.execute(
                "INSERT INTO Bible VALUES (?1,?2,?3,?4)",
                rusqlite::params![b, ch, v, t],
            )
            .unwrap();
        }
    }

    /// Replace `path` atomically with a new Bible (new inode, size and mtime).
    fn replace_bible(path: &std::path::Path, rows: &[(i32, i32, i32, &str)]) {
        let tmp = path.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp);
        make_bible(&tmp, rows);
        std::fs::rename(&tmp, path).unwrap();
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve(
        word: &str,
        dicts: &[String],
        lexs: &[String],
        il: Option<&str>,
        (b, c, v): (i32, i32, i32),
        occurrence: Option<usize>,
        typed: Option<bool>,
        bible_path: Option<&str>,
        tagged: Option<Vec<&str>>,
    ) -> WordLinkResult {
        run(resolve_word(
            word.to_string(),
            dicts.to_vec(),
            lexs.to_vec(),
            il.map(str::to_string),
            b,
            c,
            v,
            occurrence,
            typed,
            bible_path.map(str::to_string),
            tagged.map(|t| t.into_iter().map(str::to_string).collect()),
        ))
        .expect("resolve_word must not fail")
    }

    fn json<T: Serialize>(v: &T) -> serde_json::Value {
        serde_json::to_value(v).unwrap()
    }

    fn first(r: &WordLinkResult) -> &str {
        r.strongs.first().map(String::as_str).unwrap_or("")
    }

    // ─── ISS-02/35/36: optional resolve_word arguments ───

    #[test]
    fn resolve_opts_without_new_args_is_the_legacy_default() {
        assert_eq!(resolve_opts(None, None, None), ResolveOpts::default());
        // JSON null from the UI deserializes to None too.
        let v: (Option<usize>, Option<bool>, Option<String>, Option<Vec<String>>) =
            serde_json::from_str("[null, null, null, null]").unwrap();
        assert_eq!(resolve_opts(v.1, v.2, v.3), ResolveOpts::default());
        assert_eq!(v.0, None);
    }

    #[test]
    fn resolve_opts_maps_typed_tags_and_an_existing_bible() {
        let d = tmp_dir("opts");
        let bible = d.join("b.bbli");
        make_bible(&bible, &[(1, 1, 1, "En el principio")]);
        let bp = bible.to_string_lossy().to_string();
        let o = resolve_opts(
            Some(true),
            Some(bp.clone()),
            Some(vec!["H127".into(), "H859".into()]),
        );
        assert!(o.typed);
        assert_eq!(o.tagged_strongs, vec!["H127", "H859"]);
        assert_eq!(o.bible_path.as_deref(), Some(bp.as_str()));
        assert_eq!(o.primary_dictionary, None);
        assert_eq!(o.primary_lexicon, None);
        assert!(!resolve_opts(Some(false), None, None).typed);
    }

    #[test]
    fn stale_blank_or_directory_bible_path_falls_back_to_none() {
        let d = tmp_dir("stale");
        for p in [
            "/nonexistent/espada/00RVR1960.bbli".to_string(),
            d.join("borrado.bbli").to_string_lossy().to_string(),
            d.to_string_lossy().to_string(), // a folder, not a module
            String::new(),
            "   ".to_string(),
        ] {
            assert_eq!(usable_bible_path(Some(p.clone())), None, "{p:?}");
            assert_eq!(resolve_opts(None, Some(p), None), ResolveOpts::default());
        }
        assert_eq!(usable_bible_path(None), None);
    }

    #[test]
    fn tagged_strongs_from_the_webview_are_capped() {
        let many: Vec<String> = (1..=100).map(|n| format!("H{n}")).collect();
        let o = resolve_opts(None, None, Some(many));
        assert_eq!(o.tagged_strongs.len(), MAX_TAGGED_STRONGS);
        assert_eq!(o.tagged_strongs[0], "H1");
    }

    #[test]
    fn resolve_pins_interlinear_and_bible_only_when_an_interlinear_exists() {
        assert_eq!(resolve_pins(None, Some("/b.bbli")), None);
        assert_eq!(resolve_pins(Some(""), Some("/b.bbli")), None);
        assert_eq!(resolve_pins(Some("/il.bbli"), None), Some(vec!["/il.bbli".to_string()]));
        assert_eq!(
            resolve_pins(Some("/il.bbli"), Some("/b.bbli")),
            Some(vec!["/il.bbli".to_string(), "/b.bbli".to_string()])
        );
        assert_eq!(
            resolve_pins(Some("/il.bbli"), Some("/il.bbli")),
            Some(vec!["/il.bbli".to_string()])
        );
    }

    #[test]
    fn resolve_word_without_new_args_equals_the_legacy_call_no_library() {
        // No modules at all: same JSON as resolve_word_links, and no error.
        for w in ["amor", "H430", "G26", "אֱלֹהִים", ""] {
            let got = resolve(w, &[], &[], None, (43, 3, 16), None, None, None, None);
            let want = modules::resolve_word_links(w, &[], &[], None, 43, 3, 16);
            assert_eq!(json(&got), json(&want), "{w}");
        }
    }

    #[test]
    fn golden_resolve_word_without_new_args_equals_the_legacy_call() {
        let Some([il, lex, dict]) = lib_files([
            "00Interlineal-iRV 1960+.bbli",
            "03STRONGes.lexi",
            "03 Nuevo Diccionario Biblico Vila Escuain.dcti",
        ]) else {
            return;
        };
        let (dicts, lexs) = (vec![dict], vec![lex]);
        for (w, at) in [
            ("principio", (1, 1, 1)),
            ("creó", (1, 1, 1)),
            ("amó", (43, 3, 16)),
            ("que", (43, 3, 16)),
            ("Jehová", (19, 23, 1)),
            ("H430", (1, 1, 1)),
        ] {
            let got = resolve(w, &dicts, &lexs, Some(&il), at, None, None, None, None);
            let want =
                modules::resolve_word_links(w, &dicts, &lexs, Some(&il), at.0, at.1, at.2);
            assert_eq!(json(&got), json(&want), "{w}@{at:?}");
        }
        let r = resolve("principio", &dicts, &lexs, Some(&il), (1, 1, 1), None, None, None, None);
        assert_eq!(first(&r), "H7225");
    }

    #[test]
    fn golden_stale_bible_path_behaves_like_no_bible_path() {
        let Some([il]) = lib_files(["00Interlineal-iRV 1960+.bbli"]) else {
            return;
        };
        for (w, at) in [("Espíritu", (29, 2, 28)), ("principio", (1, 1, 1))] {
            let none = resolve(w, &[], &[], Some(&il), at, None, None, None, None);
            let stale = resolve(
                w,
                &[],
                &[],
                Some(&il),
                at,
                None,
                None,
                Some("/Users/nadie/Módulos viejos/AB-biblia_corona_de_jerusaln.bbli"),
                None,
            );
            assert_eq!(json(&stale), json(&none), "{w}");
        }
    }

    #[test]
    fn golden_resolve_word_forwards_bible_path_occurrence_typed_and_tags() {
        let Some([il, cj]) = lib_files([
            "00Interlineal-iRV 1960+.bbli",
            "AB-biblia_corona_de_jerusaln.bbli",
        ]) else {
            return;
        };
        // ISS-35: CJ Joel 3:1 = RV Joel 2:28 → Espíritu H7307 only with biblePath.
        let r = resolve("Espíritu", &[], &[], Some(&il), (29, 3, 1), None, None, Some(&cj), None);
        assert_eq!(first(&r), "H7307");
        assert_eq!(r.interlinear_verse, Some([29, 2, 28]));
        // ISS-02: occurrence reaches resolve_word_links_at (same answers as calling it directly).
        for occ in [0usize, 1, 2] {
            let got = resolve("que", &[], &[], Some(&il), (43, 3, 16), Some(occ), None, None, None);
            let want = resolve_word_links_at(
                "que",
                &[],
                &[],
                Some(&il),
                43,
                3,
                16,
                Some(occ),
                &ResolveOpts::default(),
            );
            assert_eq!(got.strongs, want.strongs, "occurrence {occ}");
        }
        let o0 = resolve("que", &[], &[], Some(&il), (43, 3, 16), Some(0), None, None, None);
        let o1 = resolve("que", &[], &[], Some(&il), (43, 3, 16), Some(1), None, None, None);
        assert_ne!(first(&o0), first(&o1), "occurrence ignored");
        // ISS-02: typed «Jehová» at Jn 3:16 must not become the article.
        let t = resolve("Jehová", &[], &[], Some(&il), (43, 3, 16), None, Some(true), None, None);
        assert!(t.strongs.is_empty(), "{:?}", t.strongs);
        // ISS-36: the tapped word's own tag wins.
        let g = resolve(
            "SEÑOR",
            &[],
            &[],
            Some(&il),
            (9, 16, 7),
            None,
            None,
            None,
            Some(vec!["H3068"]),
        );
        assert_eq!(first(&g), "H3068");
    }

    // ─── ISS-11: data work runs off the calling (main) thread, concurrently, without deadlock ───

    #[test]
    fn blocking_work_runs_on_another_thread() {
        let caller = std::thread::current().id();
        let worker = run(blocking(|| std::thread::current().id())).unwrap();
        assert_ne!(caller, worker);
    }

    #[test]
    fn a_panicking_lookup_becomes_an_error_not_a_crash() {
        let r: Result<(), String> = run(blocking(|| panic!("boom")));
        let e = r.unwrap_err();
        assert!(e.starts_with("Error interno"), "{e}");
    }

    #[test]
    fn parallel_module_jobs_are_capped() {
        // 40 concurrent jobs (like the side pane's Promise.all): never more than the cap at
        // once, but more than one (they do run in parallel), and all finish.
        let live = std::sync::Arc::new(AtomicUsize::new(0));
        let peak = std::sync::Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..40)
            .map(|i| {
                let (live, peak) = (live.clone(), peak.clone());
                tauri::async_runtime::spawn(blocking(move || {
                    let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(15));
                    live.fetch_sub(1, Ordering::SeqCst);
                    i
                }))
            })
            .collect();
        let sum: usize = run(async {
            let mut s = 0;
            for h in handles {
                s += h.await.unwrap().unwrap();
            }
            s
        });
        assert_eq!(sum, (0..40).sum::<usize>());
        let p = peak.load(Ordering::SeqCst);
        assert!(p <= MAX_PARALLEL_DB_JOBS, "peak {p}");
        assert!(p >= 2, "jobs never overlapped (peak {p})");
        assert!(*lock(&DB_JOBS.0) <= MAX_PARALLEL_DB_JOBS);
    }

    #[test]
    fn slow_lookup_does_not_hold_up_a_fast_one() {
        // A slow blocking job and a fast one in flight together: the fast reply arrives first.
        let (tx, rx) = std::sync::mpsc::channel::<&'static str>();
        let tx2 = tx.clone();
        let slow = tauri::async_runtime::spawn(blocking(move || {
            std::thread::sleep(Duration::from_millis(400));
            let _ = tx.send("slow");
        }));
        std::thread::sleep(Duration::from_millis(30));
        let fast = tauri::async_runtime::spawn(blocking(move || {
            let _ = tx2.send("fast");
        }));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "fast");
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "slow");
        run(async {
            slow.await.unwrap().unwrap();
            fast.await.unwrap().unwrap();
        });
    }

    #[test]
    fn golden_concurrent_commands_no_deadlock_same_results() {
        let Some([il, cmt, lex]) = lib_files([
            "00Interlineal-iRV 1960+.bbli",
            "Comentario Matthew Henry.cmti",
            "03STRONGes.lexi",
        ]) else {
            return;
        };
        let words = [
            ("principio", (1, 1, 1)),
            ("amó", (43, 3, 16)),
            ("mundo", (43, 3, 16)),
            ("Verbo", (43, 1, 1)),
        ];
        let lexs = vec![lex.clone()];
        let seq: Vec<_> = words
            .iter()
            .map(|(w, at)| json(&modules::resolve_word_links(w, &[], &lexs, Some(&il), at.0, at.1, at.2)))
            .collect();
        let seq_cmt = json(&load_commentary(&cmt, 43, 3, 16, true, false, false).unwrap());

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let (il2, cmt2, lex2) = (il.clone(), cmt.clone(), lex.clone());
        std::thread::spawn(move || {
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    let (il, cmt, lex) = (il2.clone(), cmt2.clone(), lex2.clone());
                    let (w, at) = words[i % words.len()];
                    tauri::async_runtime::spawn(async move {
                        let r = resolve_word(
                            w.to_string(),
                            vec![],
                            vec![lex.clone()],
                            Some(il),
                            at.0,
                            at.1,
                            at.2,
                            None,
                            None,
                            None,
                            None,
                        )
                        .await
                        .unwrap();
                        let c = get_commentary(
                            cmt,
                            43,
                            3,
                            16,
                            CommentaryFilters { verse: true, chapter: false, book: false },
                            None,
                        )
                        .await
                        .unwrap();
                        let l = lookup_lexicon(lex, "G26".into(), None).await.unwrap();
                        (i, json(&r), json(&c), l.is_some())
                    })
                })
                .collect();
            let out: Vec<_> = run(async {
                let mut v = Vec::new();
                for h in handles {
                    v.push(h.await.unwrap());
                }
                v
            });
            let _ = done_tx.send(out);
        });
        let out = done_rx
            .recv_timeout(Duration::from_secs(300))
            .expect("concurrent commands deadlocked (no reply in 300 s)");
        assert_eq!(out.len(), 8);
        for (i, r, c, lex_hit) in out {
            assert_eq!(r, seq[i % words.len()], "resolve result changed under concurrency ({i})");
            assert_eq!(c, seq_cmt, "commentary changed under concurrency ({i})");
            assert!(lex_hit, "G26 missing from STRONGes");
        }
    }

    // ─── ISS-46: refresh / folder switch clears the chapter cache ───

    fn verse1(state: &AppState, path: &str) -> String {
        let ch = run(cached_chapter(state, path, 43, 3)).unwrap();
        ch.verses.first().map(|v| v.plain.clone()).unwrap_or_default()
    }

    #[test]
    fn refresh_clears_cached_chapters_end_to_end() {
        let d = tmp_dir("refresh");
        let bible = d.join("b.bbli");
        make_bible(&bible, &[(43, 3, 1, "uno")]);
        let path = bible.to_string_lossy().to_string();
        let dir = d.to_string_lossy().to_string();
        let state = AppState::default();
        assert!(!state.on_list_modules(&dir), "first folder is not a switch");
        assert_eq!(verse1(&state, &path), "uno");
        // The module is replaced on disk («Actualizar lista» after copying a new version).
        replace_bible(&bible, &[(43, 3, 1, "dos versión nueva más larga")]);
        assert_eq!(verse1(&state, &path), "uno", "chapter cache should still serve the old text");
        state.on_refresh_modules(&dir);
        assert!(lock(&state.chapter_cache).map.is_empty());
        assert_eq!(verse1(&state, &path), "dos versión nueva más larga");
    }

    #[test]
    fn folder_switch_clears_chapter_cache_same_folder_does_not() {
        let state = AppState::default();
        let (g0, payload) = {
            let d = tmp_dir("switch");
            let bible = d.join("b.bbli");
            make_bible(&bible, &[(43, 3, 1, "uno")]);
            let p = bible.to_string_lossy().to_string();
            (state.chapter_get("k").1, load_bible_chapter(&p, 43, 3).unwrap())
        };
        assert!(!state.on_list_modules("/Módulos/A"));
        state.chapter_put("k".into(), payload.clone(), g0);
        assert!(!state.on_list_modules("/Módulos/A"), "same folder is not a switch");
        assert!(state.chapter_get("k").0.is_some(), "same-folder list must keep the cache");
        assert!(state.on_list_modules("/Módulos/B"), "a different folder is a switch");
        assert!(state.chapter_get("k").0.is_none());
        assert!(state.on_list_modules("/Módulos/A"), "switching back is a switch too");
    }

    #[test]
    fn a_load_started_before_a_clear_is_not_cached_after_it() {
        let d = tmp_dir("race");
        let bible = d.join("b.bbli");
        make_bible(&bible, &[(43, 3, 1, "uno")]);
        let payload = load_bible_chapter(&bible.to_string_lossy(), 43, 3).unwrap();
        let state = AppState::default();
        let (_, g_before) = state.chapter_get("k");
        state.on_refresh_modules("/Módulos/A"); // refresh lands while the load is in flight
        state.chapter_put("k".into(), payload.clone(), g_before);
        assert!(state.chapter_get("k").0.is_none(), "stale load re-filled the cache");
        let (_, g_now) = state.chapter_get("k");
        state.chapter_put("k".into(), payload, g_now);
        assert!(state.chapter_get("k").0.is_some());
    }

    #[test]
    fn chapter_cache_stays_bounded() {
        let d = tmp_dir("cap");
        let bible = d.join("b.bbli");
        make_bible(&bible, &[(43, 3, 1, "uno")]);
        let payload = load_bible_chapter(&bible.to_string_lossy(), 43, 3).unwrap();
        let state = AppState::default();
        for i in 0..200 {
            let (_, g) = state.chapter_get("x");
            state.chapter_put(format!("k{i}"), payload.clone(), g);
            assert!(lock(&state.chapter_cache).map.len() <= CHAPTER_CACHE_CAP + 1);
        }
    }

    #[test]
    fn poisoned_cache_lock_does_not_break_later_lookups() {
        let state = std::sync::Arc::new(AppState::default());
        let s2 = state.clone();
        let _ = std::thread::spawn(move || {
            let _g = s2.chapter_cache.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert!(state.chapter_cache.is_poisoned());
        assert!(state.chapter_get("k").0.is_none());
        state.on_refresh_modules("/x");
    }

    // ─── get_passage helpers (behaviour unchanged by the async move) ───

    #[test]
    fn passage_range_swaps_clips_and_caps() {
        assert_eq!(passage_range(16, None), (16, 16));
        assert_eq!(passage_range(18, Some(16)), (16, 18)); // Jn 3:18-16 → 3:16–18
        assert_eq!(passage_range(0, Some(-3)), (1, 1));
        assert_eq!(passage_range(1, Some(100)), (1, 41)); // capped at 41 verses
        assert_eq!(passage_range(100, Some(1)), (1, 41));
    }

    #[test]
    fn passage_from_chapter_clips_and_reports_empty() {
        let d = tmp_dir("passage");
        let bible = d.join("b.bbli");
        let rows: Vec<(i32, i32, i32, String)> =
            (1..=36).map(|v| (43, 3, v, format!("versículo {v}"))).collect();
        let refs: Vec<(i32, i32, i32, &str)> =
            rows.iter().map(|(b, c, v, t)| (*b, *c, *v, t.as_str())).collect();
        make_bible(&bible, &refs);
        let ch = load_bible_chapter(&bible.to_string_lossy(), 43, 3).unwrap();
        let p = passage_from_chapter(ch.clone(), 43, 3, 35, 40).ok().unwrap();
        assert_eq!((p.verse_start, p.verse_end, p.verses.len()), (35, 36, 2)); // Jn 3:35-40 → 3:35–36
        assert!(passage_from_chapter(ch, 43, 3, 50, 52).is_err());
    }

    // ─── ISS-34: navigation guard ───

    fn nav(url: &str, dev: bool) -> bool {
        is_app_navigation(&url.parse().unwrap(), dev)
    }

    #[test]
    fn nav_guard_allows_only_the_app_origin() {
        for dev in [false, true] {
            assert!(nav("tauri://localhost", dev));
            assert!(nav("tauri://localhost/index.html#libro", dev));
            assert!(nav("http://tauri.localhost/index.html", dev));
            assert!(nav("https://tauri.localhost/", dev));
            for bad in [
                "https://example.com/",
                "http://example.com/tauri.localhost",
                "http://tauri.localhost.evil.com/",
                "http://tauri.localhost:8080/",
                "tauri://evil",
                "file:///etc/passwd",
                "about:blank",
                "data:text/html,<script>alert(1)</script>",
                "javascript:alert(1)",
                "blob:tauri://localhost/1234",
                "ipc://localhost/resolve_word",
                "http://127.0.0.1:9999/",
                "mailto:a@b.c",
            ] {
                assert!(!nav(bad, dev), "{bad} (dev={dev})");
            }
        }
    }

    #[test]
    fn nav_guard_dev_server_only_in_debug_mode() {
        for u in ["http://127.0.0.1:1420/", "http://localhost:1420/index.html"] {
            assert!(nav(u, true), "{u}");
            assert!(!nav(u, false), "{u} must be blocked in release");
        }
        assert!(!nav("https://127.0.0.1:1420/", true));
    }

    // ─── ISS-34: CSP in tauri.conf.json ───

    #[test]
    fn tauri_conf_csp_has_no_inline_scripts_and_keeps_ipc() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().unwrap();
        let directive = |name: &str| -> String {
            csp.split(';')
                .map(str::trim)
                .find(|d| d.split_whitespace().next() == Some(name))
                .unwrap_or_else(|| panic!("missing {name}"))
                .to_string()
        };
        let script = directive("script-src");
        assert!(!script.contains("'unsafe-inline'") && !script.contains("'unsafe-eval'"), "{script}");
        assert!(directive("style-src").contains("'unsafe-inline'"));
        let connect = directive("connect-src");
        assert!(connect.contains("ipc:") && connect.contains("http://ipc.localhost"), "{connect}");
        let img = directive("img-src");
        assert!(img.contains("data:") && img.contains("blob:"), "{img}");
        for d in ["object-src 'none'", "base-uri 'none'", "form-action 'none'", "frame-src 'none'"] {
            assert!(csp.contains(d), "missing {d}");
        }
        assert_eq!(conf["build"]["frontendDist"], "../ui");
        assert_eq!(conf["identifier"], "com.asignaciondelcielo.espada37");
        assert_eq!(conf["app"]["withGlobalTauri"], true);
    }

    /// Bundle hygiene: frontendDist embeds everything under ui/, so tests and fixtures must
    /// live outside it, and the app's HTML must not need 'unsafe-inline' scripts.
    #[test]
    fn ui_folder_has_only_app_files_and_no_inline_scripts() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
        let mut names: Vec<String> = std::fs::read_dir(&ui)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != ".DS_Store")
            .collect();
        names.sort();
        for n in &names {
            let p = ui.join(n);
            if n == "fonts" && p.is_dir() {
                // The one allowed folder: the bundled reading fonts + their licenses (meant to
                // ship; build-release.sh whitelists it too). Nothing else may live there.
                for e in std::fs::read_dir(&p).unwrap().flatten() {
                    let f = e.file_name().to_string_lossy().to_string();
                    if f == ".DS_Store" {
                        continue;
                    }
                    assert!(e.path().is_file(), "ui/fonts/{f} is a folder: it would ship inside the app");
                    let ok = f.ends_with(".woff2") || f.ends_with(".ttf") || f.ends_with(".txt");
                    assert!(ok, "unexpected file ui/fonts/{f} would ship inside the app");
                }
                continue;
            }
            assert!(p.is_file(), "ui/{n} is a folder: it would ship inside the app");
            let ok = n.ends_with(".html") || n.ends_with(".js") || n.ends_with(".css");
            assert!(ok, "unexpected file ui/{n} would ship inside the app");
            assert!(!n.starts_with("test"), "test file ui/{n} would ship inside the app");
        }
        let html = std::fs::read_to_string(ui.join("index.html")).unwrap();
        let re_inline = regex::Regex::new(r"(?is)<script\b([^>]*)>(.*?)</script>").unwrap();
        for c in re_inline.captures_iter(&html) {
            assert!(c[1].contains("src="), "script without src in index.html");
            assert!(c[2].trim().is_empty(), "inline script body in index.html");
        }
        let re_on = regex::Regex::new(r#"(?i)<[^>]+\son[a-z]+\s*="#).unwrap();
        assert!(!re_on.is_match(&html), "inline on*= handler in index.html");
        assert!(!html.to_ascii_lowercase().contains("javascript:"));
    }
}

/// test-writer-qa additions for WP5: in-flight concurrency through the real async paths
/// (`cached_chapter`, `resolve_word`, `blocking`) on synthetic modules, so they never SKIP.
/// Every wait is bounded by a timeout, so a deadlock fails the test instead of hanging it.
/// Like `wp5_tests`, these never call the process-global `clear_caches()`.
#[cfg(test)]
mod wp5_qa_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    const DEADLOCK_TIMEOUT: Duration = Duration::from_secs(60);

    fn tmp_dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "espada-wp5qa-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_bible(path: &std::path::Path, text: &str) {
        let tmp = path.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp);
        let c = rusqlite::Connection::open(&tmp).unwrap();
        c.execute_batch(
            "CREATE TABLE Details (Title TEXT, Abbreviation TEXT, Version INT, Strongs BOOL);
             INSERT INTO Details VALUES ('Prueba','PRB',1,0);
             CREATE TABLE Bible (Book INT, Chapter INT, Verse INT, Scripture TEXT);
             CREATE INDEX BookChapterVerseIndex ON Bible (Book, Chapter, Verse);",
        )
        .unwrap();
        c.execute(
            "INSERT INTO Bible VALUES (43, 3, 16, ?1)",
            rusqlite::params![text],
        )
        .unwrap();
        drop(c);
        std::fs::rename(&tmp, path).unwrap();
    }

    /// Run `f` on its own thread; panic if it gives no answer within [`DEADLOCK_TIMEOUT`].
    fn within_timeout<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(DEADLOCK_TIMEOUT) {
            Ok(v) => v,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("{what}: no answer in {DEADLOCK_TIMEOUT:?} (deadlock?)")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("{what}: failed (assertion/panic above)")
            }
        }
    }

    fn text(ch: &ChapterPayload) -> String {
        ch.verses.first().map(|v| v.plain.clone()).unwrap_or_default()
    }

    #[test]
    fn two_lookups_in_flight_on_one_chapter_both_complete_with_one_cache_entry() {
        // Arrange: an uncached chapter, so both lookups miss and load at the same time.
        let d = tmp_dir("inflight");
        let bible = d.join("b.bbli");
        make_bible(&bible, "Porque de tal manera amó Dios al mundo");
        let path = bible.to_string_lossy().to_string();
        let state = Arc::new(AppState::default());

        // Act: two chapter lookups and a word lookup in flight together.
        let (s, p) = (state.clone(), path.clone());
        let (a, b, w) = within_timeout("two chapter lookups + resolve_word", move || {
            let spawn_chapter = |s: Arc<AppState>, p: String| {
                tauri::async_runtime::spawn(async move { cached_chapter(&s, &p, 43, 3).await })
            };
            let h1 = spawn_chapter(s.clone(), p.clone());
            let h2 = spawn_chapter(s, p);
            let h3 = tauri::async_runtime::spawn(resolve_word(
                "amó".into(),
                vec![],
                vec![],
                None,
                43,
                3,
                16,
                None,
                None,
                None,
                None,
            ));
            tauri::async_runtime::block_on(async {
                (
                    h1.await.unwrap().unwrap(),
                    h2.await.unwrap().unwrap(),
                    h3.await.unwrap().unwrap(),
                )
            })
        });

        // Assert: both chapter lookups answered with the same text, cached once.
        assert_eq!(text(&a), "Porque de tal manera amó Dios al mundo");
        assert_eq!(text(&b), text(&a));
        assert_eq!(serde_json::to_value(&w).unwrap(), {
            let legacy = modules::resolve_word_links("amó", &[], &[], None, 43, 3, 16);
            serde_json::to_value(&legacy).unwrap()
        });
        let c = lock(&state.chapter_cache);
        assert_eq!(c.map.len(), 1, "one key, one entry");
        assert!(c.map.contains_key(&chapter_key(&path, 43, 3)));
    }

    /// ISS-46 race through the real async path (the unit test calls `chapter_put` directly):
    /// a chapter load that is already in flight when «Actualizar lista» runs must not put its
    /// pre-refresh text back into the cache.
    #[test]
    fn chapter_load_in_flight_during_refresh_does_not_bring_old_text_back() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};

        let d = tmp_dir("race-async");
        let bible = d.join("b.bbli");
        let path = bible.to_string_lossy().to_string();
        let dir = d.to_string_lossy().to_string();

        // A first poll that is already Ready means the load finished before the refresh,
        // which proves nothing; retry until the load is really in flight.
        let saw_in_flight = within_timeout("load in flight across a refresh", move || {
            let mut saw_in_flight = false;
            for _ in 0..50 {
                make_bible(&bible, "texto viejo");
                let state = AppState::default();
                // Act 1: start the load (reads the cache generation, queues the SQLite job).
                let mut load = std::pin::pin!(cached_chapter(&state, &path, 43, 3));
                let mut cx = Context::from_waker(Waker::noop());
                let first = load.as_mut().poll(&mut cx);
                // Act 2: refresh lands while that load is in flight.
                state.on_refresh_modules(&dir);
                let old = match first {
                    Poll::Ready(r) => r.unwrap(),
                    Poll::Pending => {
                        saw_in_flight = true;
                        tauri::async_runtime::block_on(load).unwrap()
                    }
                };
                assert_eq!(text(&old), "texto viejo", "the in-flight caller still gets its answer");
                // Assert: nothing cached from before the refresh; the next read sees the new file.
                assert!(
                    state.chapter_get(&chapter_key(&path, 43, 3)).0.is_none(),
                    "a load that started before the refresh re-filled the cache"
                );
                make_bible(&bible, "texto nuevo después de actualizar");
                let fresh = tauri::async_runtime::block_on(cached_chapter(&state, &path, 43, 3));
                assert_eq!(text(&fresh.unwrap()), "texto nuevo después de actualizar");
                if saw_in_flight {
                    break;
                }
            }
            saw_in_flight
        });
        assert!(saw_in_flight, "the load never stayed in flight across the refresh");
    }

    /// A panicking module job must give its slot back. If it did not, after
    /// MAX_PARALLEL_DB_JOBS panics (one bad module tapped a few times) every later lookup would
    /// wait forever: the whole app would hang, not just that module.
    #[test]
    fn panicking_lookups_release_their_slot_so_later_lookups_still_run() {
        let answer = within_timeout("lookups after panics", || {
            tauri::async_runtime::block_on(async {
                for i in 0..(MAX_PARALLEL_DB_JOBS * 2) {
                    let r: Result<(), String> =
                        blocking(move || panic!("módulo dañado #{i}")).await;
                    assert!(r.is_err(), "panic #{i} must come back as an Err");
                }
                blocking(|| 42).await
            })
        });
        assert_eq!(answer, Ok(42));
    }
}
