mod esword;

use esword::{
    get_chapter, get_commentaries, list_books, list_dictionary_topics, lookup_dictionary,
    lookup_lexicon, probe_commentary, probe_dictionary, probe_lexicon, resolve_word_strongs,
    scan_modules, search_commentaries, BookInfo, ChapterResult, CommentaryResult, ContentProbe,
    DictionaryResult, LexiconResult, ModuleInfo, ResolveStrongsResult, DEFAULT_ESWORD_PATH,
};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: String,
    version: String,
    offline: bool,
    mode: String,
    esword_path: String,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Asignación del Cielo Bible".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        offline: true,
        mode: "e-sword-local".into(),
        esword_path: DEFAULT_ESWORD_PATH.into(),
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
        .unwrap_or(4);
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
    let mut out = Vec::with_capacity(paths.len());
    for h in handles {
        if let Ok(part) = h.join() {
            out.extend(part);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            suggest_dictionary_topics,
            resolve_strongs,
            probe_modules_content,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
