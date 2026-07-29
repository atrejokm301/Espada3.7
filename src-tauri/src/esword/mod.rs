//! Lectura offline de módulos e-Sword (.bbli/.bblx, .cmti/.cmtx, .dcti/.dctx, .lexi/.lexx).

mod books;
mod catalog;
mod reader;
mod text;

pub use books::{list_books, BookInfo};
pub use catalog::{scan_modules, ModuleInfo};
pub use reader::{
    get_chapter, get_commentaries, list_dictionary_topics, lookup_dictionary, lookup_lexicon,
    probe_commentary, probe_dictionary, probe_lexicon, resolve_word_strongs, search_commentaries,
    ChapterResult, CommentaryResult, ContentProbe, DictionaryResult, LexiconResult,
    ResolveStrongsResult,
};

pub const DEFAULT_ESWORD_PATH: &str = r"C:\Program Files (x86)\e-Sword";
