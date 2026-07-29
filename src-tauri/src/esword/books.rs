use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookInfo {
    pub number: i32,
    pub name: String,
    pub abbr: String,
    pub chapters: i32,
    pub testament: String,
}

/// Canon protestante 66 libros (números e-Sword).
pub fn list_books() -> Vec<BookInfo> {
    BOOKS
        .iter()
        .map(|(n, name, abbr, ch, t)| BookInfo {
            number: *n,
            name: (*name).to_string(),
            abbr: (*abbr).to_string(),
            chapters: *ch,
            testament: (*t).to_string(),
        })
        .collect()
}

pub fn book_by_number(n: i32) -> Option<BookInfo> {
    list_books().into_iter().find(|b| b.number == n)
}

pub fn book_by_name(name: &str) -> Option<BookInfo> {
    let q = normalize(name);
    list_books().into_iter().find(|b| {
        normalize(&b.name) == q
            || normalize(&b.abbr) == q
            || normalize(&b.name).starts_with(&q)
            || q.starts_with(&normalize(&b.name))
    })
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace('ú', "u")
        .replace('ñ', "n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// (number, spanish_name, abbr, chapters, testament)
const BOOKS: &[(i32, &str, &str, i32, &str)] = &[
    (1, "Génesis", "Gn", 50, "AT"),
    (2, "Éxodo", "Éx", 40, "AT"),
    (3, "Levítico", "Lv", 27, "AT"),
    (4, "Números", "Nm", 36, "AT"),
    (5, "Deuteronomio", "Dt", 34, "AT"),
    (6, "Josué", "Jos", 24, "AT"),
    (7, "Jueces", "Jue", 21, "AT"),
    (8, "Rut", "Rt", 4, "AT"),
    (9, "1 Samuel", "1Sa", 31, "AT"),
    (10, "2 Samuel", "2Sa", 24, "AT"),
    (11, "1 Reyes", "1Re", 22, "AT"),
    (12, "2 Reyes", "2Re", 25, "AT"),
    (13, "1 Crónicas", "1Cr", 29, "AT"),
    (14, "2 Crónicas", "2Cr", 36, "AT"),
    (15, "Esdras", "Esd", 10, "AT"),
    (16, "Nehemías", "Neh", 13, "AT"),
    (17, "Ester", "Est", 10, "AT"),
    (18, "Job", "Job", 42, "AT"),
    (19, "Salmos", "Sal", 150, "AT"),
    (20, "Proverbios", "Pr", 31, "AT"),
    (21, "Eclesiastés", "Ec", 12, "AT"),
    (22, "Cantares", "Cnt", 8, "AT"),
    (23, "Isaías", "Is", 66, "AT"),
    (24, "Jeremías", "Jer", 52, "AT"),
    (25, "Lamentaciones", "Lam", 5, "AT"),
    (26, "Ezequiel", "Ez", 48, "AT"),
    (27, "Daniel", "Dn", 12, "AT"),
    (28, "Oseas", "Os", 14, "AT"),
    (29, "Joel", "Jl", 3, "AT"),
    (30, "Amós", "Am", 9, "AT"),
    (31, "Abdías", "Abd", 1, "AT"),
    (32, "Jonás", "Jon", 4, "AT"),
    (33, "Miqueas", "Miq", 7, "AT"),
    (34, "Nahúm", "Nah", 3, "AT"),
    (35, "Habacuc", "Hab", 3, "AT"),
    (36, "Sofonías", "Sof", 3, "AT"),
    (37, "Hageo", "Hag", 2, "AT"),
    (38, "Zacarías", "Zac", 14, "AT"),
    (39, "Malaquías", "Mal", 4, "AT"),
    (40, "Mateo", "Mt", 28, "NT"),
    (41, "Marcos", "Mc", 16, "NT"),
    (42, "Lucas", "Lc", 24, "NT"),
    (43, "Juan", "Jn", 21, "NT"),
    (44, "Hechos", "Hch", 28, "NT"),
    (45, "Romanos", "Ro", 16, "NT"),
    (46, "1 Corintios", "1Co", 16, "NT"),
    (47, "2 Corintios", "2Co", 13, "NT"),
    (48, "Gálatas", "Ga", 6, "NT"),
    (49, "Efesios", "Ef", 6, "NT"),
    (50, "Filipenses", "Fil", 4, "NT"),
    (51, "Colosenses", "Col", 4, "NT"),
    (52, "1 Tesalonicenses", "1Ts", 5, "NT"),
    (53, "2 Tesalonicenses", "2Ts", 3, "NT"),
    (54, "1 Timoteo", "1Ti", 6, "NT"),
    (55, "2 Timoteo", "2Ti", 4, "NT"),
    (56, "Tito", "Tit", 3, "NT"),
    (57, "Filemón", "Flm", 1, "NT"),
    (58, "Hebreos", "He", 13, "NT"),
    (59, "Santiago", "Stg", 5, "NT"),
    (60, "1 Pedro", "1Pe", 5, "NT"),
    (61, "2 Pedro", "2Pe", 3, "NT"),
    (62, "1 Juan", "1Jn", 5, "NT"),
    (63, "2 Juan", "2Jn", 1, "NT"),
    (64, "3 Juan", "3Jn", 1, "NT"),
    (65, "Judas", "Jud", 1, "NT"),
    (66, "Apocalipsis", "Ap", 22, "NT"),
];
