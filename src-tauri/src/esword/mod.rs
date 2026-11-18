//! Lectura offline de módulos e-Sword (.bbli/.bblx, .cmti/.cmtx, .dcti/.dctx, .lexi/.lexx).

mod bible_titles;
mod books;
mod catalog;
mod reader;
mod sections;
mod text;
mod strong_gloss;
mod topic_index;

pub use books::{list_books, BookInfo};
pub use catalog::{scan_modules, ModuleInfo};
pub use reader::{
    get_chapter, get_commentaries, get_devotional, get_reference_chapter, list_devotional_days,
    list_dictionary_topics, list_reference_chapters, lookup_dictionary, lookup_lexicon,
    probe_commentary, probe_dictionary, probe_lexicon, resolve_word_strongs, search_bible_word,
    search_commentaries, BibleSearchHit, ChapterResult, CommentaryResult, ContentProbe,
    DevotionalResult, DictionaryResult, LexiconResult, ReferenceEntry, ReferenceResult,
    ResolveStrongsResult,
};
pub use strong_gloss::{strong_gloss, strong_lemmas, StrongGloss, StrongLemma};
pub use topic_index::{topic_entry, topic_index, IndexSlice};

pub const DEFAULT_ESWORD_PATH: &str = r"C:\Program Files (x86)\e-Sword";
