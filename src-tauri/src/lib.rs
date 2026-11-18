mod esword;
pub mod api_dispatch;

// Shared e-Sword helpers for sibling apps (e.g. e-ink Tauri host).
pub use esword::{
    get_chapter as esword_get_chapter, list_books as esword_list_books,
    scan_modules as esword_scan_modules, BookInfo, ChapterResult, ModuleInfo, DEFAULT_ESWORD_PATH,
};

use esword::{
    get_chapter, get_commentaries, get_devotional as esword_get_devotional,
    get_reference_chapter as esword_get_reference_chapter, list_books,
    list_devotional_days as esword_list_devotional_days, list_dictionary_topics,
    list_reference_chapters as esword_list_reference_chapters, lookup_dictionary, lookup_lexicon,
    probe_commentary, probe_dictionary, probe_lexicon, resolve_word_strongs, scan_modules,
    search_bible_word as esword_search_bible_word, search_commentaries,
    strong_gloss as esword_strong_gloss, strong_lemmas as esword_strong_lemmas, topic_entry,
    topic_index as esword_topic_index, BibleSearchHit, CommentaryResult, ContentProbe,
    DevotionalResult, DictionaryResult, IndexSlice, LexiconResult, ReferenceEntry,
    ReferenceResult, ResolveStrongsResult, StrongGloss, StrongLemma,
};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

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

/// A calendar day a devotional module has an entry for.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DevotionalDay {
    month: i32,
    day: i32,
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

// Every command that reads a module is `async`: it runs off the window's thread, so a slow
// read (a 280 MB commentary search ~1 s, a word that walks 74 interlinears ~0.1 s, the first
// read of a file after a reboot) can never freeze the window. The page keeps only the newest
// answer per screen (`latest()` in app.js), since answers can now arrive out of order.
#[tauri::command(async)]
fn list_esword_modules(path: Option<String>) -> Result<Vec<ModuleInfo>, String> {
    scan_modules(path)
}

#[tauri::command]
fn list_bible_books() -> Vec<BookInfo> {
    list_books()
}

#[tauri::command(async)]
fn get_bible_chapter(
    module_path: String,
    book_number: i32,
    chapter: i32,
) -> Result<ChapterResult, String> {
    get_chapter(&module_path, book_number, chapter)
}

#[tauri::command(async)]
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

// `async`: off the window's thread. A lookup can scan a whole module (a Greek word in
// Multiléxico: ~0.7 s); on the main thread that froze the window for its whole length.
#[tauri::command(async)]
fn search_dictionary(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<DictionaryResult>, String> {
    lookup_dictionary(&module_path, &term, limit.unwrap_or(20))
}

// `async`: see search_dictionary.
#[tauri::command(async)]
fn search_lexicon(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<LexiconResult>, String> {
    lookup_lexicon(&module_path, &term, limit.unwrap_or(20))
}

#[tauri::command(async)]
fn search_commentary_term(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<CommentaryResult>, String> {
    search_commentaries(&module_path, &term, limit.unwrap_or(30))
}

/// Concordance / word cross-references in the selected Bible module.
#[tauri::command(async)]
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

// `async`: see search_dictionary.
#[tauri::command(async)]
fn suggest_dictionary_topics(
    module_path: String,
    term: String,
    limit: Option<i32>,
) -> Result<Vec<DictionaryResult>, String> {
    list_dictionary_topics(&module_path, &term, limit.unwrap_or(40))
}

/// Índice of a dictionary or lexicon (e-Sword style): a window of its alphabetical topic
/// list around `query` (or `anchor`), with where the query is and whether it is a topic.
///
/// `async`: a sync command runs on the main thread, and the first window of a big module
/// builds its index (webster-1913: 115 837 topics) — that must not freeze the window.
#[tauri::command(async)]
fn topic_index(
    module_path: String,
    query: Option<String>,
    anchor: Option<u32>,
    before: Option<u32>,
    after: Option<u32>,
) -> Result<IndexSlice, String> {
    esword_topic_index(
        &module_path,
        query.as_deref(),
        anchor,
        before.unwrap_or(40),
        after.unwrap_or(80),
    )
}

/// The article of one index row: exactly that topic, by its rowid.
#[tauri::command(async)]
fn get_topic_entry(module_path: String, rowid: i64) -> Result<Option<DictionaryResult>, String> {
    topic_entry(&module_path, rowid)
}

/// The Spanish renderings a Spanish Strong's dictionary lists for `code` («G26» → «ágape,
/// amado, amor»), from the first of `module_paths` with a readable entry: the «ES» line of a
/// number looked up by hand, clicked or opened from the Índice, where no Bible word gives one.
#[tauri::command(async)]
fn strong_gloss(code: String, module_paths: Vec<String>) -> Option<StrongGloss> {
    esword_strong_gloss(&code, &module_paths)
}

/// The original word of every entry of the given Spanish Strong's dictionaries, for the page's
/// Greek/Hebrew word → Strong's number table (each module read once, then kept).
#[tauri::command(async)]
fn strong_lemmas(module_paths: Vec<String>) -> Vec<StrongLemma> {
    esword_strong_lemmas(&module_paths)
}

// Command names must match the `api_dispatch` match arms exactly: the front end
// sends the same string to Tauri IPC and to the WinUI HTTP sidecar.
#[tauri::command(async)]
fn list_reference_chapters(module_path: String) -> Result<Vec<ReferenceEntry>, String> {
    esword_list_reference_chapters(&module_path)
}

#[tauri::command(async)]
fn get_reference_chapter(
    module_path: String,
    index: i64,
) -> Result<Option<ReferenceResult>, String> {
    esword_get_reference_chapter(&module_path, index)
}

#[tauri::command(async)]
fn list_devotional_days(module_path: String) -> Result<Vec<DevotionalDay>, String> {
    Ok(esword_list_devotional_days(&module_path)?
        .into_iter()
        .map(|(month, day)| DevotionalDay { month, day })
        .collect())
}

#[tauri::command(async)]
fn get_devotional(
    module_path: String,
    month: i32,
    day: i32,
) -> Result<Option<DevotionalResult>, String> {
    esword_get_devotional(&module_path, month, day)
}

#[tauri::command(async)]
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

/// One counter per kind, bumped by every probe of that kind: a running probe stops once a
/// newer one of the same kind starts. The UI keeps only the newest probe's answer, so the
/// rest of an older one is wasted work.
static PROBE_GEN: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

fn probe_lane(kind: &str) -> &'static AtomicU64 {
    &PROBE_GEN[match kind {
        "commentary" => 0,
        "dictionary" => 1,
        "lexicon" => 2,
        _ => 3,
    }]
}

/// Commentaries are probed per verse and there can be hundreds after a mass import, so they
/// stay capped. Dictionaries and lexicons are probed in full: a cap hid the ● of every
/// module past it (86 lexicons and 351 dictionaries are installed here).
const MAX_COMMENTARY_PROBE: usize = 48;

/// The ● markers: which of `paths` have content for this verse or term.
///
/// `async`: probing all 351 dictionaries takes about a second, and a sync command runs on
/// the main thread, which would freeze the window on every search.
#[tauri::command(async)]
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
    let paths: Vec<String> = if kind == "commentary" {
        paths.into_iter().take(MAX_COMMENTARY_PROBE).collect()
    } else {
        paths
    };
    let lane = probe_lane(&kind);
    let gen = lane.fetch_add(1, Ordering::Relaxed) + 1;
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
        .map(|n| n.get().min(8).max(2))
        .unwrap_or(2)
        .min(paths.len());
    // Each worker takes the next module as it frees up, so one huge module (webster-1913)
    // does not hold back a fixed chunk of others.
    let total = paths.len();
    let paths = Arc::new(paths);
    let next = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..workers {
        let paths = paths.clone();
        let next = next.clone();
        let kind = kind.clone();
        let term = term.clone();
        handles.push(std::thread::spawn(move || {
            let mut part = Vec::new();
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(p) = paths.get(i) else { break };
                if lane.load(Ordering::Relaxed) != gen {
                    break;
                }
                // Never let one bad module panic the worker thread
                let probe = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
                    path: p.clone(),
                    has_content: false,
                    level: String::new(),
                    note: "probe panic".into(),
                });
                part.push((i, probe));
            }
            part
        }));
    }
    let mut out = Vec::with_capacity(total);
    for h in handles {
        match h.join() {
            Ok(part) => out.extend(part),
            Err(_) => {
                write_crash_log("probe worker thread panicked (join err)");
            }
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, probe)| probe).collect()
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
            topic_index,
            get_topic_entry,
            strong_gloss,
            strong_lemmas,
            list_reference_chapters,
            get_reference_chapter,
            list_devotional_days,
            get_devotional,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(exts: &[&str]) -> Vec<String> {
        let Ok(dir) = std::fs::read_dir(DEFAULT_ESWORD_PATH) else {
            return Vec::new();
        };
        let mut v: Vec<String> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                exts.contains(&ext.as_str())
            })
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn probe(kind: &str, paths: &[String], term: &str) -> Vec<ContentProbe> {
        probe_modules_content(kind.into(), paths.to_vec(), 1, 1, 1, Some(term.into()), None, None, None)
    }

    /// Every installed lexicon is probed, not the first 48, and the ● agrees with the lookup:
    /// a lexicon is marked exactly when searching it finds the code (na26- stores G00026).
    #[test]
    fn every_lexicon_is_probed_and_marked_when_its_lookup_finds_the_code() {
        let lexicons = installed(&["lexi", "lexx"]);
        if lexicons.len() <= MAX_COMMENTARY_PROBE {
            eprintln!("skip: needs more than {MAX_COMMENTARY_PROBE} installed lexicons");
            return;
        }
        let probes = probe("lexicon", &lexicons, "G26");
        assert_eq!(probes.len(), lexicons.len(), "a probe was cut short");
        for (p, path) in probes.iter().zip(&lexicons) {
            assert_eq!(&p.path, path);
            let found = lookup_lexicon(path, "G26", 20).map(|v| !v.is_empty()).unwrap_or(false);
            assert_eq!(p.has_content, found, "● disagrees with the lookup for {path}");
        }
        let marked = probes.iter().filter(|p| p.has_content).count();
        assert!(marked > MAX_COMMENTARY_PROBE, "only {marked} lexicons marked for G26");
        assert!(
            probes.iter().any(|p| p.has_content && p.path.ends_with("na26-.lexi")),
            "na26- (G00026) not marked"
        );
    }

    #[test]
    fn every_dictionary_is_probed_but_commentaries_stay_capped() {
        let dictionaries = installed(&["dcti", "dctx"]);
        if dictionaries.len() > MAX_COMMENTARY_PROBE {
            assert_eq!(probe("dictionary", &dictionaries, "amor").len(), dictionaries.len());
        }
        let commentaries = installed(&["cmti", "cmtx"]);
        if commentaries.len() > MAX_COMMENTARY_PROBE {
            assert_eq!(probe("commentary", &commentaries, "").len(), MAX_COMMENTARY_PROBE);
        }
    }

    /// How long each command takes with Kevin's modules (Romanos 8:24, RV1960, MacArthur,
    /// Multiléxico, Thompson): first call (connection opened) and second. Report only, release:
    /// `cargo test --release --lib -- --ignored perf_report --nocapture`.
    #[test]
    #[ignore]
    fn perf_report() {
        use std::path::Path;
        use std::time::Instant;
        let dir = Path::new(DEFAULT_ESWORD_PATH);
        if !dir.is_dir() {
            return;
        }
        let p = |f: &str| dir.join(f).to_string_lossy().into_owned();
        let t_scan = Instant::now();
        let mods = scan_modules(None).unwrap();
        eprintln!("{:<44} {:>9.1} ms", "list_esword_modules (cached scan)", t_scan.elapsed().as_secs_f64() * 1000.0);
        let of = |kind: &str| -> Vec<String> {
            mods.iter().filter(|m| m.module_type == kind).map(|m| m.path.clone()).collect()
        };
        // The UI's interlinear choice (interlinearCandidates): Bibles marked as interlinear or
        // Strong's-tagged, best first.
        let mut inter: Vec<(i32, String)> = mods
            .iter()
            .filter(|m| m.module_type == "bible")
            .filter_map(|m| {
                let hay = format!("{} {} {}", m.title, m.filename, m.abbreviation).to_lowercase();
                if !(hay.contains("interlin") || hay.contains("strong") || m.filename.contains('+')) {
                    return None;
                }
                let mut score = 0;
                if hay.contains("interlin") { score += 50; }
                if hay.contains("1960") || hay.contains("irv") { score += 30; }
                if hay.contains("griego") || hay.contains("greek") { score += 10; }
                Some((score, m.path.clone()))
            })
            .collect();
        inter.sort_by(|a, b| b.0.cmp(&a.0));
        let inter: Vec<String> = inter.into_iter().map(|(_, s)| s).collect();
        let bible = p("rv1960_reina_valera_1960.bbli");
        let cmt = p("macarthur_biblia_de_estudio_de_macarthur.cmti");
        let multi = p("01Multilexico.lexi");
        let thompson = p("02THOMPSON.dcti");
        let refs = of("reference");
        let devs = of("devotional");
        let cmts = of("commentary");
        let dicts = of("dictionary");
        let lexs = of("lexicon");
        let time = |name: &str, f: &mut dyn FnMut() -> usize| {
            let t0 = Instant::now();
            let n = f();
            let a = t0.elapsed().as_secs_f64() * 1000.0;
            let t1 = Instant::now();
            f();
            let b = t1.elapsed().as_secs_f64() * 1000.0;
            eprintln!("{name:<44} {a:>9.1} ms  then {b:>8.1} ms   ({n} results)");
        };
        time("get_bible_chapter Ro 8", &mut || get_chapter(&bible, 45, 8).map(|c| c.verses.len()).unwrap_or(0));
        time("get_verse_commentaries Ro 8:24", &mut || get_commentaries(&cmt, 45, 8, 24, true, false, false).map(|v| v.len()).unwrap_or(0));
        time("resolve_strongs «esperanza» Ro 8:24", &mut || {
            resolve_word_strongs(45, 8, 24, "esperanza", -1, &inter).map(|r| r.hits.len()).unwrap_or(0)
        });
        time("search_bible_word «esperanza»", &mut || {
            esword_search_bible_word(&bible, "esperanza", Some(45), Some(8), Some(24), 40).map(|v| v.len()).unwrap_or(0)
        });
        time("search_commentary_term «esperanza»", &mut || search_commentaries(&cmt, "esperanza", 30).map(|v| v.len()).unwrap_or(0));
        if let Some(r) = refs.first() {
            time("list_reference_chapters (first module)", &mut || esword_list_reference_chapters(r).map(|v| v.len()).unwrap_or(0));
            time("get_reference_chapter 0", &mut || esword_get_reference_chapter(r, 0).ok().flatten().map(|_| 1).unwrap_or(0));
        }
        if let Some(d) = devs.first() {
            time("list_devotional_days (first module)", &mut || esword_list_devotional_days(d).map(|v| v.len()).unwrap_or(0));
            time("get_devotional 11/18", &mut || esword_get_devotional(d, 11, 18).ok().flatten().map(|_| 1).unwrap_or(0));
        }
        time("search_lexicon Multiléxico G5569", &mut || lookup_lexicon(&multi, "G5569", 20).map(|v| v.len()).unwrap_or(0));
        time("search_lexicon Multiléxico «ψευδάδελφος»", &mut || lookup_lexicon(&multi, "ψευδάδελφος", 20).map(|v| v.len()).unwrap_or(0));
        time("suggest_dictionary_topics Thompson «dios»", &mut || list_dictionary_topics(&thompson, "dios", 40).map(|v| v.len()).unwrap_or(0));
        time("topic_index Multiléxico around G5569", &mut || esword_topic_index(&multi, Some("G5569"), None, 40, 80).map(|s| s.rows.len()).unwrap_or(0));
        time("probe 48 commentaries Ro 8:24", &mut || probe_modules_content("commentary".into(), cmts.clone(), 45, 8, 24, None, None, None, None).len());
        time("probe all lexicons G5569", &mut || probe_modules_content("lexicon".into(), lexs.clone(), 1, 1, 1, Some("G5569".into()), None, None, None).len());
        time("probe all dictionaries «esperanza»", &mut || probe_modules_content("dictionary".into(), dicts.clone(), 1, 1, 1, Some("esperanza".into()), None, None, None).len());
        time("get_bible_chapter Ro 8 after the probes", &mut || get_chapter(&bible, 45, 8).map(|c| c.verses.len()).unwrap_or(0));
        time("search_lexicon Multiléxico after the probes", &mut || lookup_lexicon(&multi, "G5569", 20).map(|v| v.len()).unwrap_or(0));
        eprintln!("interlinear candidates: {}", inter.len());

        // Worst cases: words that do not resolve, the biggest commentary, no-match searches.
        eprintln!("--- worst cases");
        time("resolve_strongs «creación» Ro 8:19", &mut || {
            resolve_word_strongs(45, 8, 19, "creación", -1, &inter).map(|r| r.hits.len()).unwrap_or(0)
        });
        time("resolve_strongs «xyzzy» Ro 8:24 (no such word)", &mut || {
            resolve_word_strongs(45, 8, 24, "xyzzy", -1, &inter).map(|r| r.hits.len()).unwrap_or(0)
        });
        let mut by_size: Vec<(u64, String)> = cmts
            .iter()
            .map(|c| (std::fs::metadata(c).map(|m| m.len()).unwrap_or(0), c.clone()))
            .collect();
        by_size.sort();
        if let Some((size, big)) = by_size.last() {
            eprintln!("biggest commentary: {} ({} MB)", Path::new(big).file_name().unwrap().to_string_lossy(), size / 1_000_000);
            time("get_verse_commentaries (biggest) Ro 8:24 +ch +book", &mut || get_commentaries(big, 45, 8, 24, true, true, true).map(|v| v.len()).unwrap_or(0));
            time("search_commentary_term (biggest) «gracia»", &mut || search_commentaries(big, "gracia", 30).map(|v| v.len()).unwrap_or(0));
            time("search_commentary_term (biggest) no match", &mut || search_commentaries(big, "xyzzyq", 30).map(|v| v.len()).unwrap_or(0));
        }
        time("search_bible_word no match", &mut || {
            esword_search_bible_word(&bible, "xyzzyq", None, None, None, 40).map(|v| v.len()).unwrap_or(0)
        });
        let mut dict_sizes: Vec<(u64, String)> = dicts
            .iter()
            .map(|c| (std::fs::metadata(c).map(|m| m.len()).unwrap_or(0), c.clone()))
            .collect();
        dict_sizes.sort();
        if let Some((size, big)) = dict_sizes.last() {
            eprintln!("biggest dictionary: {} ({} MB)", Path::new(big).file_name().unwrap().to_string_lossy(), size / 1_000_000);
            time("suggest_dictionary_topics (biggest) «amor»", &mut || list_dictionary_topics(big, "amor", 40).map(|v| v.len()).unwrap_or(0));
            time("suggest_dictionary_topics (biggest) no match", &mut || list_dictionary_topics(big, "xyzzyq", 40).map(|v| v.len()).unwrap_or(0));
        }
    }
}
