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
///
/// Includes e-Sword's own Spanish (Jua, Apo, Ose, Hec, 1Te, 2Te, Age, Stgo) and English
/// (Jdg, Rth, Ecc, Son, Oba, Zep, Luk, Jam) abbreviations used in module `<ref>` tags.
/// Deuterocanonical tokens (Eco = Eclesiástico, Jdt = Judit, Sab, Sir, Tob, Bar, 1Ma, 2Ma,
/// 1Es, 2Es) are deliberately NOT mapped: the 66-book table has no slot for them, and mapping
/// them to a canonical book would open the wrong passage.
/// Keys are matched exactly (after `normalize_book_key`), so a key must appear under one book only.
pub fn book_aliases() -> Vec<(Vec<&'static str>, i32)> {
    vec![
        (vec!["genesis", "gen", "gn", "ge", "gén", "génesis"], 1),
        (vec!["exodo", "ex", "exo", "exod", "éx", "éxodo"], 2),
        (vec!["levitico", "lev", "lv", "le", "levítico"], 3),
        (vec!["numeros", "num", "nm", "nu", "números", "núm"], 4),
        (vec!["deuteronomio", "deut", "dt", "de", "deu"], 5),
        (vec!["josue", "jos", "jo", "josué"], 6),
        (vec!["jueces", "jue", "juec", "ju", "jdc", "jdg", "judg"], 7),
        (vec!["rut", "rt", "ru", "rth"], 8),
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
        (vec!["eclesiastes", "ecl", "ec", "qo", "eclesiastés", "ecls", "ecc", "eccl"], 21),
        (vec!["cantares", "cant", "cnt", "ct", "song", "can", "cantardelos", "son", "sng"], 22),
        (vec!["isaias", "isa", "is", "isaías"], 23),
        (vec!["jeremias", "jer", "je", "jr", "jeremías"], 24),
        (vec!["lamentaciones", "lam", "lm", "lament"], 25),
        (vec!["ezequiel", "eze", "ez", "ezq", "ezeq"], 26),
        (vec!["daniel", "dan", "dn", "da"], 27),
        (vec!["oseas", "os", "hos", "oséas", "ose"], 28),
        (vec!["joel", "jl", "joe"], 29),
        (vec!["amos", "am", "amo", "amós"], 30),
        (vec!["abdias", "abd", "ob", "abdías", "oba", "obad"], 31),
        (vec!["jonas", "jon", "jnh", "jonás"], 32),
        (vec!["miqueas", "miq", "mi", "mic"], 33),
        (vec!["nahum", "nah", "na", "nahúm"], 34),
        (vec!["habacuc", "hab", "ha"], 35),
        (vec!["sofonias", "sof", "zp", "sofonías", "zep", "zeph"], 36),
        (vec!["hageo", "hag", "hg", "age", "ageo"], 37),
        (vec!["zacarias", "zac", "zc", "zec", "zacarías"], 38),
        (vec!["malaquias", "mal", "ml", "malaquías"], 39),
        (vec!["mateo", "mat", "mt", "ma"], 40),
        (vec!["marcos", "mar", "mr", "mc", "mk"], 41),
        (vec!["lucas", "luc", "lc", "lk", "lu", "luk", "luke"], 42),
        (vec!["juan", "jn", "joh", "jhn", "jua"], 43),
        (vec!["hechos", "hech", "hch", "act", "ac", "hecho", "hec"], 44),
        (vec!["romanos", "rom", "ro", "rm"], 45),
        (vec!["1corintios", "1cor", "1co", "1c"], 46),
        (vec!["2corintios", "2cor", "2co", "2c"], 47),
        (vec!["galatas", "gal", "ga", "gl", "gálatas", "gá"], 48),
        (vec!["efesios", "ef", "eph", "efes", "efe"], 49),
        (vec!["filipenses", "fil", "php", "flp"], 50),
        (vec!["colosenses", "col", "co"], 51),
        (vec!["1tesalonicenses", "1tes", "1ts", "1th", "1tesal", "1te"], 52),
        (vec!["2tesalonicenses", "2tes", "2ts", "2th", "2tesal", "2te"], 53),
        (vec!["1timoteo", "1tim", "1ti", "1tm"], 54),
        (vec!["2timoteo", "2tim", "2ti", "2tm"], 55),
        (vec!["tito", "tit", "ti", "tt"], 56),
        (vec!["filemon", "flm", "flmón", "phm", "filemón"], 57),
        (vec!["hebreos", "heb", "he", "hbr"], 58),
        (vec!["santiago", "sant", "stg", "st", "jas", "jac", "jam", "james", "stgo", "snt"], 59),
        (vec!["1pedro", "1ped", "1p", "1pe", "1pt"], 60),
        (vec!["2pedro", "2ped", "2p", "2pe", "2pt"], 61),
        (vec!["1juan", "1jn", "1jo", "1j", "ijuan"], 62),
        (vec!["2juan", "2jn", "2jo", "2j", "iijuan"], 63),
        (vec!["3juan", "3jn", "3jo", "3j", "iiijuan"], 64),
        (vec!["judas", "jud", "jude", "jd"], 65),
        (vec!["apocalipsis", "apoc", "ap", "rev", "re", "apo"], 66),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// e-Sword's own English book abbreviations, books 1..66 in canonical order.
    const ESWORD_EN: &str = "Gen Exo Lev Num Deu Jos Jdg Rth 1Sa 2Sa 1Ki 2Ki 1Ch 2Ch Ezr Neh Est Job Psa Pro Ecc Son Isa Jer Lam Eze Dan Hos Joe Amo Oba Jon Mic Nah Hab Zep Hag Zec Mal Mat Mar Luk Joh Act Rom 1Co 2Co Gal Eph Php Col 1Th 2Th 1Ti 2Ti Tit Phm Heb Jas 1Pe 2Pe 1Jn 2Jn 3Jn Jud Rev";
    /// e-Sword's own Spanish book abbreviations, books 1..66 in canonical order.
    const ESWORD_ES: &str = "Gén Éxo Lev Núm Deu Jos Jue Rut 1Sa 2Sa 1Re 2Re 1Cr 2Cr Esd Neh Est Job Sal Pro Ecl Cnt Isa Jer Lam Eze Dan Ose Joe Amó Abd Jon Miq Nah Hab Sof Hag Zac Mal Mat Mar Luc Jua Hch Rom 1Co 2Co Gál Efe Fil Col 1Ts 2Ts 1Ti 2Ti Tit Flm Heb Stg 1Pe 2Pe 1Jn 2Jn 3Jn Jud Apo";

    /// Deuterocanonical / apocryphal tokens seen in the library. The 66-book table has
    /// no slot for them, so they must stay unresolved. Mapping them would open the WRONG passage:
    /// Eco = Eclesiástico (not Eclesiastés), Jdt = Judit (not Judas), 1Es = 1 Esdras (not Esdras).
    const DEUTERO: &[&str] = &[
        "Eco", "Sir", "Sab", "Wis", "Tob", "Jdt", "Bar", "1Ma", "2Ma", "3Ma", "4Ma", "1Es", "2Es",
        "3Es", "4Es", "3Esd", "1En", "Man",
    ];

    /// Frozen snapshot of the alias table as it was before ISS-06 (v3.7.0). Every key here
    /// must keep resolving to the same book after any alias addition.
    const LEGACY_ALIASES: &[(i32, &str)] = &[
        (1, "genesis gen gn ge gén génesis"),
        (2, "exodo ex exo exod éx éxodo"),
        (3, "levitico lev lv le levítico"),
        (4, "numeros num nm nu números núm"),
        (5, "deuteronomio deut dt de deu"),
        (6, "josue jos jo josué"),
        (7, "jueces jue juec ju jdc"),
        (8, "rut rt ru"),
        (9, "1samuel 1sam 1s 1sa ism"),
        (10, "2samuel 2sam 2s 2sa iism"),
        (11, "1reyes 1re 1r 1ki 1rey 1rs"),
        (12, "2reyes 2re 2r 2ki 2rey 2rs"),
        (13, "1cronicas 1cr 1cron 1ch 1cro 1crónicas"),
        (14, "2cronicas 2cr 2cron 2ch 2cro 2crónicas"),
        (15, "esdras esd ezr"),
        (16, "nehemias neh ne nehemías"),
        (17, "ester est es"),
        (18, "job jb"),
        (19, "salmos salmo sal ps psa psalm sl"),
        (20, "proverbios prov pro pr prv"),
        (21, "eclesiastes ecl ec qo eclesiastés ecls"),
        (22, "cantares cant cnt ct song can cantardelos"),
        (23, "isaias isa is isaías"),
        (24, "jeremias jer je jr jeremías"),
        (25, "lamentaciones lam lm lament"),
        (26, "ezequiel eze ez ezq ezeq"),
        (27, "daniel dan dn da"),
        (28, "oseas os hos oséas"),
        (29, "joel jl joe"),
        (30, "amos am amo amós"),
        (31, "abdias abd ob abdías"),
        (32, "jonas jon jnh jonás"),
        (33, "miqueas miq mi mic"),
        (34, "nahum nah na nahúm"),
        (35, "habacuc hab ha"),
        (36, "sofonias sof zp sofonías"),
        (37, "hageo hag hg"),
        (38, "zacarias zac zc zec zacarías"),
        (39, "malaquias mal ml malaquías"),
        (40, "mateo mat mt ma"),
        (41, "marcos mar mr mc mk"),
        (42, "lucas luc lc lk lu"),
        (43, "juan jn joh jhn"),
        (44, "hechos hech hch act ac hecho"),
        (45, "romanos rom ro rm"),
        (46, "1corintios 1cor 1co 1c"),
        (47, "2corintios 2cor 2co 2c"),
        (48, "galatas gal ga gl gálatas gá"),
        (49, "efesios ef eph efes efe"),
        (50, "filipenses fil php flp"),
        (51, "colosenses col co"),
        (52, "1tesalonicenses 1tes 1ts 1th 1tesal"),
        (53, "2tesalonicenses 2tes 2ts 2th 2tesal"),
        (54, "1timoteo 1tim 1ti 1tm"),
        (55, "2timoteo 2tim 2ti 2tm"),
        (56, "tito tit ti tt"),
        (57, "filemon flm flmón phm filemón"),
        (58, "hebreos heb he hbr"),
        (59, "santiago sant stg st jas jac"),
        (60, "1pedro 1ped 1p 1pe 1pt"),
        (61, "2pedro 2ped 2p 2pe 2pt"),
        (62, "1juan 1jn 1jo 1j ijuan"),
        (63, "2juan 2jn 2jo 2j iijuan"),
        (64, "3juan 3jn 3jo 3j iiijuan"),
        (65, "judas jud jude jd"),
        (66, "apocalipsis apoc ap rev re"),
    ];

    fn assert_set_in_order(set: &str) {
        let toks: Vec<&str> = set.split_whitespace().collect();
        assert_eq!(toks.len(), 66, "abbreviation set must have 66 entries");
        let bad: Vec<String> = toks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let want = i as i32 + 1;
                let got = resolve_book_name(t);
                (got != Some(want)).then(|| format!("{t}: want {want}, got {got:?}"))
            })
            .collect();
        assert!(bad.is_empty(), "unresolved/misresolved e-Sword tokens: {bad:?}");
    }

    #[test]
    fn esword_english_abbreviations_resolve_in_order() {
        assert_set_in_order(ESWORD_EN);
    }

    #[test]
    fn esword_spanish_abbreviations_resolve_in_order() {
        assert_set_in_order(ESWORD_ES);
    }

    #[test]
    fn esword_variant_abbreviations_resolve() {
        let cases: &[(&str, i32)] = &[
            ("Jua", 43), ("jua", 43), ("Apo", 66), ("Ose", 28), ("Hec", 44),
            ("Luk", 42), ("Luke", 42), ("Jdg", 7), ("Judg", 7), ("Rth", 8),
            ("Ecc", 21), ("Eccl", 21), ("Son", 22), ("Sng", 22), ("Oba", 31), ("Obad", 31),
            ("Zep", 36), ("Zeph", 36), ("Age", 37), ("Ageo", 37),
            ("Jam", 59), ("James", 59), ("Stgo", 59), ("Snt", 59),
            ("1Te", 52), ("2Te", 53), ("1 Te", 52), ("2 Te.", 53), ("Flp", 50),
        ];
        for (tok, want) in cases {
            assert_eq!(resolve_book_name(tok), Some(*want), "token {tok:?}");
        }
    }

    #[test]
    fn deuterocanonical_tokens_stay_unresolved() {
        for tok in DEUTERO {
            assert_eq!(resolve_book_name(tok), None, "deuterocanonical {tok:?} must not map to a canonical book");
        }
    }

    #[test]
    fn lowercase_accented_tokens_resolve() {
        // Needed by ISS-29: the NC-STRONG-E split-entity repair yields lowercase data-book values.
        for (tok, want) in [("gén", 1), ("éxo", 2), ("núm", 4), ("gál", 48), ("amós", 30), ("amó", 30), ("GÉN", 1)] {
            assert_eq!(resolve_book_name(tok), Some(want), "token {tok:?}");
        }
    }

    #[test]
    fn existing_common_forms_still_resolve() {
        for (tok, want) in [
            ("Juan", 43), ("Jn", 43), ("Ap", 66), ("Os", 28), ("Lc", 42), ("Apoc", 66),
            ("1 Co", 46), ("1 Cor", 46), ("2 Ti", 55), ("Éx", 2), ("Gál", 48), ("Hch", 44),
            ("Mc", 41), ("Zec", 38), ("Re", 66), ("1Re", 11), ("Co", 51), ("He", 58), ("Ti", 56),
            ("Gén.", 1), ("1 Juan", 62), ("Cantares", 22),
        ] {
            assert_eq!(resolve_book_name(tok), Some(want), "token {tok:?}");
        }
        for tok in ["", "  ", "xyz"] {
            assert_eq!(resolve_book_name(tok), None, "token {tok:?}");
        }
    }

    #[test]
    fn legacy_aliases_keep_their_resolution() {
        let mut bad = Vec::new();
        for (num, keys) in LEGACY_ALIASES {
            for k in keys.split_whitespace() {
                if resolve_book_name(k) != Some(*num) {
                    bad.push(format!("{k}: want {num}, got {:?}", resolve_book_name(k)));
                }
            }
        }
        assert!(bad.is_empty(), "legacy alias resolution changed: {bad:?}");
    }

    #[test]
    fn every_alias_resolves_to_its_own_book_and_no_key_is_shared() {
        use std::collections::HashMap;
        let mut owner: HashMap<String, i32> = HashMap::new();
        let mut bad = Vec::new();
        for (aliases, num) in book_aliases() {
            for a in aliases {
                let k = normalize_book_key(a);
                if let Some(prev) = owner.insert(k.clone(), num) {
                    if prev != num {
                        bad.push(format!("{a:?} listed under {prev} and {num}"));
                    }
                }
                if resolve_book_name(a) != Some(num) {
                    bad.push(format!("{a:?} under {num} resolves to {:?}", resolve_book_name(a)));
                }
            }
        }
        assert!(bad.is_empty(), "alias collisions: {bad:?}");
    }

    #[test]
    fn full_names_and_display_abbrs_resolve() {
        for b in BOOKS {
            assert_eq!(resolve_book_name(b.name), Some(b.number), "name {}", b.name);
            assert_eq!(resolve_book_name(b.abbr), Some(b.number), "abbr {}", b.abbr);
        }
    }

    // ---- QA edge cases (ISS-06 / ISS-29 test_plan + must_not_break) ----

    /// Spelling variants a module (or the JS bridge) can hand to `resolve_book_ref` for the
    /// same e-Sword token: case, trailing period, spaced / NBSP / dotted numeric prefix,
    /// surrounding whitespace.
    fn spelling_variants(tok: &str) -> Vec<String> {
        let mut v = vec![
            tok.to_string(),
            tok.to_lowercase(),
            tok.to_uppercase(),
            format!("{tok}."),
            format!("  {tok}\t"),
        ];
        if let Some(d) = tok.chars().next().filter(|c| c.is_ascii_digit()) {
            let rest = &tok[1..];
            v.push(format!("{d} {rest}"));
            v.push(format!("{d}\u{a0}{rest}"));
            v.push(format!("{d}. {rest}."));
        }
        v
    }

    #[test]
    fn esword_sets_resolve_under_every_spelling_variant() {
        let mut bad = Vec::new();
        for set in [ESWORD_EN, ESWORD_ES] {
            for (i, tok) in set.split_whitespace().enumerate() {
                let want = Some(i as i32 + 1);
                for v in spelling_variants(tok) {
                    if resolve_book_name(&v) != want {
                        bad.push(format!("{v:?}: want {want:?}, got {:?}", resolve_book_name(&v)));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "variant spellings misresolved: {bad:?}");
    }

    #[test]
    fn accent_stripped_spanish_esword_set_resolves_in_order() {
        // must_not_break: "resolve_book_ref accepts ... accent-stripped e-Sword tokens".
        let stripped: String = ESWORD_ES
            .split_whitespace()
            .map(|t| {
                t.chars()
                    .map(|c| match c {
                        'á' => 'a', 'é' => 'e', 'í' => 'i', 'ó' => 'o', 'ú' => 'u',
                        'Á' => 'A', 'É' => 'E', 'Í' => 'I', 'Ó' => 'O', 'Ú' => 'U',
                        c => c,
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(stripped.contains("Gen Exo") && stripped.contains("Amo") && stripped.contains("Gal"));
        assert_set_in_order(&stripped);
    }

    #[test]
    fn decomposed_unicode_accents_resolve_like_precomposed() {
        // NFD text (combining U+0301), as produced by macOS file names / some module exports.
        for (tok, want) in [
            ("Ge\u{301}n", 1), ("E\u{301}xo", 2), ("Nu\u{301}m", 4), ("Ga\u{301}l", 48),
            ("Amo\u{301}s", 30), ("ge\u{301}n", 1), ("Ge\u{301}nesis", 1),
        ] {
            assert_eq!(resolve_book_name(tok), Some(want), "NFD token {tok:?}");
        }
    }

    #[test]
    fn previously_dead_tokens_resolve_in_any_case() {
        // Regression for ISS-06: these 16 tokens were dead links (≈228k refs) before the fix.
        for (tok, want) in [
            ("Jua", 43), ("Apo", 66), ("Ose", 28), ("Hec", 44), ("Luk", 42), ("Jdg", 7),
            ("Rth", 8), ("Ecc", 21), ("Son", 22), ("Oba", 31), ("Zep", 36), ("Age", 37),
            ("Jam", 59), ("Stgo", 59), ("1Te", 52), ("2Te", 53),
        ] {
            for v in [tok.to_string(), tok.to_uppercase(), tok.to_lowercase(), format!("{tok}.")] {
                assert_eq!(resolve_book_name(&v), Some(want), "token {v:?}");
            }
        }
    }

    #[test]
    fn deuterocanonical_tokens_stay_unresolved_under_every_spelling() {
        // Deferred product question: the 66-book table has no slot for these books, so every
        // spelling must stay None (never open Eclesiastés for Eclesiástico, Judas for Judit...).
        let mut bad = Vec::new();
        let extra = [
            "Eclo", "Si", "Sb", "Tb", "Ba", "1M", "2M", "1Mac", "2Mac", "1 Mac.",
            "Tobías", "Judit", "Sabiduría", "Eclesiástico", "Baruc", "1 Macabeos", "2 Macabeos",
            "Sirach", "Tobit", "Judith", "Wisdom", "Baruch", "1Esd", "2Esd", "Ecclus",
        ];
        for tok in DEUTERO.iter().copied().chain(extra) {
            for v in spelling_variants(tok) {
                if let Some(b) = resolve_book_name(&v) {
                    bad.push(format!("{v:?} -> {b}"));
                }
            }
        }
        assert!(bad.is_empty(), "deuterocanonical spellings resolved to canonical books: {bad:?}");
    }

    #[test]
    fn near_miss_and_malformed_tokens_do_not_resolve() {
        // Matching is exact (after normalisation): no prefix / fuzzy hits, no bare numbers.
        for tok in [
            "Juanito", "Lucass", "Apocalipsis2", "Jn3", "4 Juan", "4Jn", "0Jn", "1", "66", "1 ",
            ".", "...", "-", "_", "<ref>", "J", "Ap0", "Psalms", "Jho", "Ezekiel", "1Col",
        ] {
            assert_eq!(resolve_book_name(tok), None, "token {tok:?} must not resolve");
        }
        let long = "a".repeat(10_000);
        assert_eq!(resolve_book_name(&long), None, "very long input");
    }

    #[test]
    fn resolution_is_stable_under_normalize_book_key() {
        // Property: normalising first never changes the answer (the JS bridge and crossrefs.rs
        // pass raw tokens; other callers may pass already-normalised keys).
        let mut bad = Vec::new();
        let names = BOOKS.iter().flat_map(|b| [b.name, b.abbr]);
        let aliases = book_aliases().into_iter().flat_map(|(a, _)| a);
        let esword = ESWORD_EN.split_whitespace().chain(ESWORD_ES.split_whitespace());
        for tok in names.chain(aliases).chain(esword) {
            let norm = normalize_book_key(tok);
            if resolve_book_name(&norm) != resolve_book_name(tok) {
                bad.push(format!("{tok:?} vs {norm:?}"));
            }
            if normalize_book_key(&norm) != norm {
                bad.push(format!("normalize not idempotent for {tok:?}"));
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn every_resolution_is_a_real_book() {
        for (aliases, num) in book_aliases() {
            assert!((1..=66).contains(&num), "alias row {aliases:?} has book {num}");
            let b = book_by_number(num).expect("book_by_number for an alias target");
            assert_eq!(b.number, num);
        }
        for n in [0, -1, 67, i32::MAX, i32::MIN] {
            assert!(book_by_number(n).is_none(), "book_by_number({n})");
        }
        assert_eq!(BOOKS.len(), 66);
        assert_eq!(BOOKS.iter().map(|b| b.chapters).sum::<i32>(), 1189);
        assert_eq!(books_json().as_array().map(|a| a.len()), Some(66));
    }

    #[test]
    fn rendered_ref_spans_carry_a_resolvable_data_book() {
        // End-to-end in Rust: module <ref> → text::clean_esword_text → span data-book
        // → resolve_book_name (what lib.rs resolve_book_ref and crossrefs.rs:264 do).
        use once_cell::sync::Lazy;
        use regex::Regex;
        static RE_BOOK: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"class="bible-ref" data-book="([^"]*)""#).unwrap());
        let cases: &[(&str, i32)] = &[
            ("<ref>Jua 3:16</ref>", 43), ("<ref>Apo 22:21</ref>", 66), ("<ref>Ose 11:1</ref>", 28),
            ("<ref>Hec 2:38</ref>", 44), ("<ref>Luk 2:14</ref>", 42), ("<ref>Jdg 13:22</ref>", 7),
            ("<ref>Rth 1:16</ref>", 8), ("<ref>Ecc 12:13</ref>", 21), ("<ref>Son 2:1</ref>", 22),
            ("<ref>Oba 1:1</ref>", 31), ("<ref>Zep 3:17</ref>", 36), ("<ref>Age 2:9</ref>", 37),
            ("<ref>Jam 1:5</ref>", 59), ("<ref>1Te 4:16</ref>", 52), ("<ref>2Te 3:10</ref>", 53),
            ("<ref>Stgo 2:17</ref>", 59), ("<ref>G&eacute;n 1:1</ref>", 1),
            ("<ref>Jua. 1:1-3</ref>", 43), ("<ref>1 Te 5:17</ref>", 52), ("<REF>apo 1:8</REF>", 66),
        ];
        for (src, want) in cases {
            let html = crate::text::clean_esword_text(src);
            let book = RE_BOOK
                .captures(&html)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| panic!("{src:?} rendered without a parsed bible-ref span: {html}"));
            assert_eq!(resolve_book_name(&book), Some(*want), "{src:?} → data-book {book:?}");
        }
        // Deuterocanonical refs still render as spans but must not resolve.
        let html = crate::text::clean_esword_text("<ref>Eco 1:1</ref>");
        if let Some(c) = RE_BOOK.captures(&html) {
            assert_eq!(resolve_book_name(&c[1]), None, "Eco must stay unresolved: {html}");
        }
    }

    // ---- Library-backed gate (slow; opt-in) ----

    fn library_dir() -> Option<std::path::PathBuf> {
        let dir = std::env::var_os("ESPADA_MODULE_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| std::path::PathBuf::from(h).join("Downloads/for mac espada"))
            })?;
        dir.is_dir().then_some(dir)
    }

    /// Open a module strictly read-only (immutable: SQLite never writes journals or -shm).
    fn open_ro(path: &std::path::Path) -> Option<rusqlite::Connection> {
        let mut enc = String::from("file:");
        for ch in path.to_string_lossy().chars() {
            match ch {
                ' ' => enc.push_str("%20"),
                '%' => enc.push_str("%25"),
                '?' => enc.push_str("%3F"),
                '#' => enc.push_str("%23"),
                c => enc.push(c),
            }
        }
        enc.push_str("?mode=ro&immutable=1");
        rusqlite::Connection::open_with_flags(
            enc,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .ok()
    }

    #[derive(Default)]
    struct RefTally {
        parsed: u64,
        unresolved: u64,
        unresolved_deutero: u64,
        tokens: std::collections::HashMap<String, u64>,
        /// Memoised resolve_book_name per distinct token (keeps debug-mode scans fast).
        cache: std::collections::HashMap<String, bool>,
    }

    impl RefTally {
        /// Same extraction as text.rs (RE_REF_TAG → strip tags → RE_PARSE book group).
        fn scan(&mut self, s: &str) {
            use once_cell::sync::Lazy;
            use regex::Regex;
            static RE_REF: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"(?i)<ref(?:\s[^>]*)?>(.*?)</ref>").unwrap());
            static RE_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
            static RE_PARSE: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r"(?i)^\s*((?:[123]\s*)?[A-Za-zÁÉÍÓÚáéíóúñÑ\.]+)\s+(\d{1,3})\s*[:：\.]\s*(\d{1,3})(?:\s*[-–—]\s*(\d{1,3}))?\s*$").unwrap()
            });
            if !s.contains("<ref") && !s.contains("<REF") && !s.contains("<Ref") {
                return;
            }
            for c in RE_REF.captures_iter(s) {
                let inner = RE_TAGS.replace_all(&c[1], "").replace('\n', " ");
                let Some(p) = RE_PARSE.captures(inner.trim()) else { continue };
                self.parsed += 1;
                let book = p[1].trim().trim_end_matches('.');
                let resolved = match self.cache.get(book) {
                    Some(r) => *r,
                    None => {
                        let r = resolve_book_name(book).is_some();
                        self.cache.insert(book.to_string(), r);
                        r
                    }
                };
                if !resolved {
                    self.unresolved += 1;
                    let k = normalize_book_key(book);
                    if DEUTERO.iter().any(|d| normalize_book_key(d) == k) {
                        self.unresolved_deutero += 1;
                    }
                    *self.tokens.entry(book.to_string()).or_default() += 1;
                }
            }
        }

        fn top(&self, n: usize) -> Vec<(String, u64)> {
            let mut v: Vec<_> = self.tokens.iter().map(|(k, c)| (k.clone(), *c)).collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            v.truncate(n);
            v
        }
    }

    fn value_text(v: rusqlite::types::ValueRef<'_>) -> Option<String> {
        match v {
            rusqlite::types::ValueRef::Text(b) | rusqlite::types::ValueRef::Blob(b) => {
                Some(String::from_utf8_lossy(b).into_owned())
            }
            _ => None,
        }
    }

    /// ISS-06 ship gate: every <ref> book token in the real library resolves, except the
    /// deuterocanonical ones. Run with: ESPADA_SLOW_TESTS=1 cargo test --lib --release
    /// books::tests::library_ref_book_tokens_resolve -- --nocapture
    #[test]
    fn library_ref_book_tokens_resolve() {
        if std::env::var_os("ESPADA_SLOW_TESTS").is_none() {
            eprintln!("SKIP books::library_ref_book_tokens_resolve: set ESPADA_SLOW_TESTS=1 to scan the module library");
            return;
        }
        let Some(dir) = library_dir() else {
            eprintln!("SKIP books::library_ref_book_tokens_resolve: module library not found (~/Downloads/for mac espada or ESPADA_MODULE_DIR)");
            return;
        };
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        files.sort();
        let ext = |p: &std::path::Path| {
            p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
        };
        let mut cmt = RefTally::default();
        let mut lexdict = RefTally::default();
        for path in &files {
            let e = ext(path);
            let tally = match e.as_str() {
                "cmti" | "cmtx" => &mut cmt,
                "lexi" | "dcti" => &mut lexdict,
                _ => continue,
            };
            let Some(conn) = open_ro(path) else { continue };
            let tables: Vec<String> = match conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table'")
                .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0))?.collect())
            {
                Ok(t) => t,
                Err(_) => continue, // encrypted / unreadable module
            };
            // Commentaries: the same tables/column the auditors' baseline scanned (refscan.py).
            // Lexicons/dictionaries: every text column of every table.
            let queries: Vec<String> = if e.starts_with("cmt") {
                if !tables.iter().any(|t| t == "VerseCommentary") {
                    continue;
                }
                ["VerseCommentary", "ChapterCommentary", "BookCommentary"]
                    .iter()
                    .filter(|t| tables.iter().any(|x| x == *t))
                    .map(|t| format!("SELECT Comments FROM \"{t}\""))
                    .collect()
            } else {
                tables.iter().map(|t| format!("SELECT * FROM \"{}\"", t.replace('"', "\"\""))).collect()
            };
            for q in queries {
                let Ok(mut st) = conn.prepare(&q) else { continue };
                let ncol = st.column_count();
                let Ok(mut rows) = st.query([]) else { continue };
                while let Ok(Some(row)) = rows.next() {
                    for i in 0..ncol {
                        if let Ok(v) = row.get_ref(i) {
                            if let Some(s) = value_text(v) {
                                tally.scan(&s);
                            }
                        }
                    }
                }
            }
        }
        for (label, t) in [("commentaries (.cmti/.cmtx)", &cmt), ("lexicons+dictionaries (.lexi/.dcti)", &lexdict)] {
            eprintln!(
                "GATE ISS-06 {label}: parsed <ref> = {}, unresolved = {} ({:.3}%), of which deuterocanonical = {}",
                t.parsed,
                t.unresolved,
                100.0 * t.unresolved as f64 / t.parsed.max(1) as f64,
                t.unresolved_deutero
            );
            eprintln!("  top unresolved tokens: {:?}", t.top(40));
        }
        // After ISS-06 the only unresolved refs should be deuterocanonical plus a long tail of
        // typos/garbage. Allow a 0.05% tail of non-deutero leftovers.
        for t in [&cmt, &lexdict] {
            let non_deutero = t.unresolved - t.unresolved_deutero;
            assert!(
                (non_deutero as f64) <= 0.0005 * t.parsed as f64,
                "too many non-deuterocanonical unresolved refs: {non_deutero} of {} (top: {:?})",
                t.parsed,
                t.top(20)
            );
        }
        for tok in ["Jua", "Apo", "Ose", "Hec", "Luk", "Jdg", "Rth", "Ecc", "Son", "Oba", "Zep", "Age", "Jam", "1Te", "2Te", "jua"] {
            assert!(!cmt.tokens.contains_key(tok) && !lexdict.tokens.contains_key(tok), "{tok} still unresolved");
        }
    }

    /// Cheap library property check (runs by default when the library is present): the four
    /// cross-reference sources the panel reads (TSKe, NASB, TCB, Thompson) must have no
    /// non-deuterocanonical <ref> book token left unresolved beyond a tiny typo tail, and none
    /// of the ISS-06 tokens. These are the sources that silently dropped 5–7% of hits.
    #[test]
    fn library_crossref_sources_have_no_dead_book_tokens() {
        let Some(dir) = library_dir() else {
            eprintln!("SKIP books::library_crossref_sources_have_no_dead_book_tokens: module library not found (~/Downloads/for mac espada or ESPADA_MODULE_DIR)");
            return;
        };
        let mut scanned = 0;
        for name in ["tske_v12.cmti", "nasb.cmti", "02TCB.cmti", "02THOMPSON.cmti"] {
            let path = dir.join(name);
            if !path.is_file() {
                eprintln!("SKIP {name}: not in library");
                continue;
            }
            let Some(conn) = open_ro(&path) else {
                eprintln!("SKIP {name}: cannot open read-only");
                continue;
            };
            let Ok(mut st) = conn.prepare("SELECT Comments FROM VerseCommentary") else {
                eprintln!("SKIP {name}: no VerseCommentary table");
                continue;
            };
            let mut t = RefTally::default();
            let mut rows = st.query([]).unwrap();
            while let Ok(Some(row)) = rows.next() {
                if let Some(s) = row.get_ref(0).ok().and_then(value_text) {
                    t.scan(&s);
                }
            }
            scanned += 1;
            let non_deutero = t.unresolved - t.unresolved_deutero;
            eprintln!(
                "{name}: parsed {} refs, unresolved {} (deutero {}), top {:?}",
                t.parsed, t.unresolved, t.unresolved_deutero, t.top(8)
            );
            assert!(t.parsed > 1000, "{name}: only {} refs parsed — extraction broken?", t.parsed);
            assert!(
                (non_deutero as f64) <= 0.0005 * t.parsed as f64,
                "{name}: {non_deutero} non-deuterocanonical unresolved refs of {} (top {:?})",
                t.parsed,
                t.top(20)
            );
            for tok in ["Jua", "Apo", "Ose", "Hec", "Luk", "Jdg", "Rth", "Ecc", "Son", "Oba", "Zep", "Age", "Jam", "1Te", "2Te"] {
                assert!(!t.tokens.contains_key(tok), "{name}: {tok} still unresolved");
            }
        }
        if scanned == 0 {
            eprintln!("SKIP books::library_crossref_sources_have_no_dead_book_tokens: none of the cross-reference modules were readable");
        }
    }
}
