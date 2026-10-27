//! Spanish book names, abbreviations, and chapter counts (Protestant 66).

#[derive(Clone, Copy)]
pub struct BookInfo {
    pub number: i32,
    pub name: &'static str,
    pub abbr: &'static str,
    pub chapters: i32,
}

pub const BOOKS: &[BookInfo] = &[
    BookInfo { number: 1, name: "Génesis", abbr: "Gn", chapters: 50 },
    BookInfo { number: 2, name: "Éxodo", abbr: "Éx", chapters: 40 },
    BookInfo { number: 3, name: "Levítico", abbr: "Lv", chapters: 27 },
    BookInfo { number: 4, name: "Números", abbr: "Nm", chapters: 36 },
    BookInfo { number: 5, name: "Deuteronomio", abbr: "Dt", chapters: 34 },
    BookInfo { number: 6, name: "Josué", abbr: "Jos", chapters: 24 },
    BookInfo { number: 7, name: "Jueces", abbr: "Jue", chapters: 21 },
    BookInfo { number: 8, name: "Rut", abbr: "Rt", chapters: 4 },
    BookInfo { number: 9, name: "1 Samuel", abbr: "1S", chapters: 31 },
    BookInfo { number: 10, name: "2 Samuel", abbr: "2S", chapters: 24 },
    BookInfo { number: 11, name: "1 Reyes", abbr: "1R", chapters: 22 },
    BookInfo { number: 12, name: "2 Reyes", abbr: "2R", chapters: 25 },
    BookInfo { number: 13, name: "1 Crónicas", abbr: "1Cr", chapters: 29 },
    BookInfo { number: 14, name: "2 Crónicas", abbr: "2Cr", chapters: 36 },
    BookInfo { number: 15, name: "Esdras", abbr: "Esd", chapters: 10 },
    BookInfo { number: 16, name: "Nehemías", abbr: "Neh", chapters: 13 },
    BookInfo { number: 17, name: "Ester", abbr: "Est", chapters: 10 },
    BookInfo { number: 18, name: "Job", abbr: "Job", chapters: 42 },
    BookInfo { number: 19, name: "Salmos", abbr: "Sal", chapters: 150 },
    BookInfo { number: 20, name: "Proverbios", abbr: "Pr", chapters: 31 },
    BookInfo { number: 21, name: "Eclesiastés", abbr: "Ec", chapters: 12 },
    BookInfo { number: 22, name: "Cantares", abbr: "Cnt", chapters: 8 },
    BookInfo { number: 23, name: "Isaías", abbr: "Is", chapters: 66 },
    BookInfo { number: 24, name: "Jeremías", abbr: "Jer", chapters: 52 },
    BookInfo { number: 25, name: "Lamentaciones", abbr: "Lm", chapters: 5 },
    BookInfo { number: 26, name: "Ezequiel", abbr: "Ez", chapters: 48 },
    BookInfo { number: 27, name: "Daniel", abbr: "Dn", chapters: 12 },
    BookInfo { number: 28, name: "Oseas", abbr: "Os", chapters: 14 },
    BookInfo { number: 29, name: "Joel", abbr: "Jl", chapters: 3 },
    BookInfo { number: 30, name: "Amós", abbr: "Am", chapters: 9 },
    BookInfo { number: 31, name: "Abdías", abbr: "Abd", chapters: 1 },
    BookInfo { number: 32, name: "Jonás", abbr: "Jon", chapters: 4 },
    BookInfo { number: 33, name: "Miqueas", abbr: "Mi", chapters: 7 },
    BookInfo { number: 34, name: "Nahúm", abbr: "Nah", chapters: 3 },
    BookInfo { number: 35, name: "Habacuc", abbr: "Hab", chapters: 3 },
    BookInfo { number: 36, name: "Sofonías", abbr: "Sof", chapters: 3 },
    BookInfo { number: 37, name: "Hageo", abbr: "Hag", chapters: 2 },
    BookInfo { number: 38, name: "Zacarías", abbr: "Zac", chapters: 14 },
    BookInfo { number: 39, name: "Malaquías", abbr: "Mal", chapters: 4 },
    BookInfo { number: 40, name: "Mateo", abbr: "Mt", chapters: 28 },
    BookInfo { number: 41, name: "Marcos", abbr: "Mr", chapters: 16 },
    BookInfo { number: 42, name: "Lucas", abbr: "Lc", chapters: 24 },
    BookInfo { number: 43, name: "Juan", abbr: "Jn", chapters: 21 },
    BookInfo { number: 44, name: "Hechos", abbr: "Hch", chapters: 28 },
    BookInfo { number: 45, name: "Romanos", abbr: "Ro", chapters: 16 },
    BookInfo { number: 46, name: "1 Corintios", abbr: "1Co", chapters: 16 },
    BookInfo { number: 47, name: "2 Corintios", abbr: "2Co", chapters: 13 },
    BookInfo { number: 48, name: "Gálatas", abbr: "Gá", chapters: 6 },
    BookInfo { number: 49, name: "Efesios", abbr: "Ef", chapters: 6 },
    BookInfo { number: 50, name: "Filipenses", abbr: "Fil", chapters: 4 },
    BookInfo { number: 51, name: "Colosenses", abbr: "Col", chapters: 4 },
    BookInfo { number: 52, name: "1 Tesalonicenses", abbr: "1Ts", chapters: 5 },
    BookInfo { number: 53, name: "2 Tesalonicenses", abbr: "2Ts", chapters: 3 },
    BookInfo { number: 54, name: "1 Timoteo", abbr: "1Ti", chapters: 6 },
    BookInfo { number: 55, name: "2 Timoteo", abbr: "2Ti", chapters: 4 },
    BookInfo { number: 56, name: "Tito", abbr: "Tit", chapters: 3 },
    BookInfo { number: 57, name: "Filemón", abbr: "Flm", chapters: 1 },
    BookInfo { number: 58, name: "Hebreos", abbr: "He", chapters: 13 },
    BookInfo { number: 59, name: "Santiago", abbr: "Stg", chapters: 5 },
    BookInfo { number: 60, name: "1 Pedro", abbr: "1P", chapters: 5 },
    BookInfo { number: 61, name: "2 Pedro", abbr: "2P", chapters: 3 },
    BookInfo { number: 62, name: "1 Juan", abbr: "1Jn", chapters: 5 },
    BookInfo { number: 63, name: "2 Juan", abbr: "2Jn", chapters: 1 },
    BookInfo { number: 64, name: "3 Juan", abbr: "3Jn", chapters: 1 },
    BookInfo { number: 65, name: "Judas", abbr: "Jud", chapters: 1 },
    BookInfo { number: 66, name: "Apocalipsis", abbr: "Ap", chapters: 22 },
];

/// Alias table for reference parsing (lowercase, no accents).
pub fn book_aliases() -> Vec<(Vec<&'static str>, i32)> {
    vec![
        (vec!["genesis", "gen", "gn", "ge", "gén", "génesis"], 1),
        (vec!["exodo", "ex", "exo", "exod", "éx", "éxodo"], 2),
        (vec!["levitico", "lev", "lv", "le", "levítico"], 3),
        (vec!["numeros", "num", "nm", "nu", "números", "núm"], 4),
        (vec!["deuteronomio", "deut", "dt", "de", "deu"], 5),
        (vec!["josue", "jos", "jo", "josué"], 6),
        (vec!["jueces", "jue", "juec", "ju", "jdc"], 7),
        (vec!["rut", "rt", "ru"], 8),
        (vec!["1samuel", "1sam", "1s", "1sa", "ism"], 9),
        (vec!["2samuel", "2sam", "2s", "2sa", "iism"], 10),
        (vec!["1reyes", "1re", "1r", "1ki", "1rey", "1rs"], 11),
        (vec!["2reyes", "2re", "2r", "2ki", "2rey", "2rs"], 12),
        (vec!["1cronicas", "1cr", "1cron", "1ch", "1cro", "1crónicas"], 13),
        (vec!["2cronicas", "2cr", "2cron", "2ch", "2cro", "2crónicas"], 14),
        (vec!["esdras", "esd", "ezr"], 15),
        (vec!["nehemias", "neh", "ne", "nehemías"], 16),
        (vec!["ester", "est", "es"], 17),
        (vec!["job", "jb"], 18),
        (vec!["salmos", "salmo", "sal", "ps", "psa", "psalm", "sl"], 19),
        (vec!["proverbios", "prov", "pro", "pr", "prv"], 20),
        (vec!["eclesiastes", "ecl", "ec", "qo", "eclesiastés", "ecls"], 21),
        (vec!["cantares", "cant", "cnt", "ct", "song", "can", "cantardelos"], 22),
        (vec!["isaias", "isa", "is", "isaías"], 23),
        (vec!["jeremias", "jer", "je", "jr", "jeremías"], 24),
        (vec!["lamentaciones", "lam", "lm", "lament"], 25),
        (vec!["ezequiel", "eze", "ez", "ezq", "ezeq"], 26),
        (vec!["daniel", "dan", "dn", "da"], 27),
        (vec!["oseas", "os", "hos", "oséas"], 28),
        (vec!["joel", "jl", "joe"], 29),
        (vec!["amos", "am", "amo", "amós"], 30),
        (vec!["abdias", "abd", "ob", "abdías"], 31),
        (vec!["jonas", "jon", "jnh", "jonás"], 32),
        (vec!["miqueas", "miq", "mi", "mic"], 33),
        (vec!["nahum", "nah", "na", "nahúm"], 34),
        (vec!["habacuc", "hab", "ha"], 35),
        (vec!["sofonias", "sof", "zp", "sofonías"], 36),
        (vec!["hageo", "hag", "hg"], 37),
        (vec!["zacarias", "zac", "zc", "zec", "zacarías"], 38),
        (vec!["malaquias", "mal", "ml", "malaquías"], 39),
        (vec!["mateo", "mat", "mt", "ma"], 40),
        (vec!["marcos", "mar", "mr", "mc", "mk"], 41),
        (vec!["lucas", "luc", "lc", "lk", "lu"], 42),
        (vec!["juan", "jn", "joh", "jhn"], 43),
        (vec!["hechos", "hech", "hch", "act", "ac", "hecho"], 44),
        (vec!["romanos", "rom", "ro", "rm"], 45),
        (vec!["1corintios", "1cor", "1co", "1c"], 46),
        (vec!["2corintios", "2cor", "2co", "2c"], 47),
        (vec!["galatas", "gal", "ga", "gl", "gálatas", "gá"], 48),
        (vec!["efesios", "ef", "eph", "efes", "efe"], 49),
        (vec!["filipenses", "fil", "php", "flp"], 50),
        (vec!["colosenses", "col", "co"], 51),
        (vec!["1tesalonicenses", "1tes", "1ts", "1th", "1tesal"], 52),
        (vec!["2tesalonicenses", "2tes", "2ts", "2th", "2tesal"], 53),
        (vec!["1timoteo", "1tim", "1ti", "1tm"], 54),
        (vec!["2timoteo", "2tim", "2ti", "2tm"], 55),
        (vec!["tito", "tit", "ti", "tt"], 56),
        (vec!["filemon", "flm", "flmón", "phm", "filemón"], 57),
        (vec!["hebreos", "heb", "he", "hbr"], 58),
        (vec!["santiago", "sant", "stg", "st", "jas", "jac"], 59),
        (vec!["1pedro", "1ped", "1p", "1pe", "1pt"], 60),
        (vec!["2pedro", "2ped", "2p", "2pe", "2pt"], 61),
        (vec!["1juan", "1jn", "1jo", "1j", "ijuan"], 62),
        (vec!["2juan", "2jn", "2jo", "2j", "iijuan"], 63),
        (vec!["3juan", "3jn", "3jo", "3j", "iiijuan"], 64),
        (vec!["judas", "jud", "jude", "jd"], 65),
        (vec!["apocalipsis", "apoc", "ap", "rev", "re"], 66),
    ]
}

#[allow(dead_code)]
pub fn book_by_number(n: i32) -> Option<&'static BookInfo> {
    BOOKS.iter().find(|b| b.number == n)
}

pub fn resolve_book_name(raw: &str) -> Option<i32> {
    let n = normalize_book_key(raw);
    if n.is_empty() {
        return None;
    }
    for (aliases, num) in book_aliases() {
        if aliases.iter().any(|a| normalize_book_key(a) == n) {
            return Some(num);
        }
    }
    // Full Spanish name match
    for b in BOOKS {
        if normalize_book_key(b.name) == n {
            return Some(b.number);
        }
    }
    None
}

pub fn normalize_book_key(s: &str) -> String {
    let s = s.trim().to_lowercase();
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'á' | 'à' | 'ä' | 'â' => out.push('a'),
            'é' | 'è' | 'ë' | 'ê' => out.push('e'),
            'í' | 'ì' | 'ï' | 'î' => out.push('i'),
            'ó' | 'ò' | 'ö' | 'ô' => out.push('o'),
            'ú' | 'ù' | 'ü' | 'û' => out.push('u'),
            'ñ' => out.push('n'),
            ' ' | '.' | '_' | '-' => {}
            c if c.is_alphanumeric() => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn books_json() -> serde_json::Value {
    let list: Vec<serde_json::Value> = BOOKS
        .iter()
        .map(|b| {
            serde_json::json!({
                "number": b.number,
                "name": b.name,
                "abbr": b.abbr,
                "chapters": b.chapters,
            })
        })
        .collect();
    serde_json::Value::Array(list)
}
