//! Spanish verb forms, for one question only: is a tapped word a form of a given infinitive?
//!
//! The Strong bridge (modules.rs) asks it with the infinitive glosses of the tapped word's own
//! Strong family (G622 → «destruir, perder, perecer»), so a dictionary article is reached from
//! stem-changing and irregular forms too: pierda → perder → PERDICIÓN, murió → morir → MUERTO,
//! creyó → creer → CREER. Generation is deliberately generous (every stem change is tried on
//! every verb and person): a form a verb does not really have can only match a word whose own
//! Strong family already lists that verb, and a miss only means "no bridge", as before.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Lowercase with acute / grave / diaeresis / circumflex removed; ñ kept (the bridge's folding).
pub fn fold(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            c => c,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Ar,
    Er,
    Ir,
}

/// Folded infinitive without a reflexive «-se» (arrepentirse → arrepentir).
fn base_infinitive(inf: &str) -> String {
    let f = fold(inf);
    match f.strip_suffix("se").filter(|s| s.ends_with('r')) {
        Some(s) => s.to_string(),
        None => f,
    }
}

fn split(base: &str) -> Option<(String, Class)> {
    if base.chars().count() < 2 || !base.chars().all(char::is_alphabetic) {
        return None;
    }
    let (stem, class) = if let Some(s) = base.strip_suffix("ar") {
        (s, Class::Ar)
    } else if let Some(s) = base.strip_suffix("er") {
        (s, Class::Er)
    } else if let Some(s) = base.strip_suffix("ir") {
        (s, Class::Ir)
    } else {
        return None;
    };
    Some((stem.to_string(), class))
}

/// A one-word gloss that looks like an infinitive (amar, perder, arrepentirse, oír).
pub fn is_infinitive(word: &str) -> bool {
    let b = base_infinitive(word);
    b.chars().count() >= 2 && split(&b).is_some()
}

// Endings without accents (forms are compared folded).
const AR_END: &[&str] = &[
    "o", "as", "a", "amos", "ais", "an", // presente
    "e", "aste", "asteis", "aron", // pretérito (o, amos above)
    "aba", "abas", "abamos", "abais", "aban", // imperfecto
    "es", "emos", "eis", "en", // presente de subjuntivo (e above)
    "ara", "aras", "aramos", "arais", "aran", "ase", "ases", "asemos", "aseis", "asen",
    "are", "ares", "aremos", "areis", "aren", // futuro de subjuntivo (RV: «amare»)
    "ad", "ando", "ado", "ada", "ados", "adas", "ar",
];
const ER_END: &[&str] = &[
    "o", "es", "e", "emos", "eis", "en", // presente
    "i", "iste", "io", "imos", "isteis", "ieron", // pretérito
    "ia", "ias", "iamos", "iais", "ian", // imperfecto
    "a", "as", "amos", "ais", "an", // presente de subjuntivo
    "iera", "ieras", "ieramos", "ierais", "ieran", "iese", "ieses", "iesemos", "ieseis", "iesen",
    "iere", "ieres", "ieremos", "iereis", "ieren", // futuro de subjuntivo (RV: «creyere»)
    "ed", "iendo", "ido", "ida", "idos", "idas", "er",
];
const IR_END: &[&str] = &[
    "o", "es", "e", "imos", "is", "en", // presente
    "i", "iste", "io", "isteis", "ieron", // pretérito (imos above)
    "ia", "ias", "iamos", "iais", "ian", // imperfecto
    "a", "as", "amos", "ais", "an", // presente de subjuntivo
    "iera", "ieras", "ieramos", "ierais", "ieran", "iese", "ieses", "iesemos", "ieseis", "iesen",
    "iere", "ieres", "ieremos", "iereis", "ieren", // futuro de subjuntivo
    "id", "iendo", "ido", "ida", "idos", "idas", "ir",
];
/// Futuro and condicional, added to the infinitive (or an irregular future stem).
const FUT: &[&str] = &["e", "as", "a", "emos", "eis", "an", "ia", "ias", "iamos", "iais", "ian"];
/// Pretérito fuerte (tuv-, pus-, dij-) and the subjunctives built on it.
const PRET_STRONG: &[&str] = &[
    "e", "iste", "o", "imos", "isteis", "ieron", "iera", "ieras", "ieramos", "ierais", "ieran",
    "iese", "ieses", "iesemos", "ieseis", "iesen", "iere", "ieres", "ieremos", "iereis", "ieren",
];
/// Presente in -go / -zc and its subjunctive (tengo, tenga; conduzco, conduzca).
const S1_END: &[&str] = &["o", "a", "as", "amos", "ais", "an"];

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// The stem with every stem change tried on its last vowel: e → ie / i, o → ue / u, u → ue,
/// i → ie (the silent u of -gu / -qu is skipped: seguir → sigu-, querer → quier-).
fn stem_variants(stem: &str) -> Vec<String> {
    let chars: Vec<char> = stem.chars().collect();
    let mut out = vec![stem.to_string()];
    let mut i = chars.len();
    while i > 0 {
        i -= 1;
        // The u of «gu»/«qu» is silent when the stem ends there or a vowel follows it.
        let silent_u = chars[i] == 'u'
            && i > 0
            && matches!(chars[i - 1], 'g' | 'q')
            && (i + 1 == chars.len() || is_vowel(chars[i + 1]));
        if is_vowel(chars[i]) && !silent_u {
            let swaps: &[&str] = match chars[i] {
                'e' => &["ie", "i"],
                'o' => &["ue", "u"],
                'u' => &["ue"],
                'i' => &["ie"],
                _ => &[],
            };
            for sw in swaps {
                let mut v: String = chars[..i].iter().collect();
                v.push_str(sw);
                v.extend(&chars[i + 1..]);
                out.push(v);
            }
            break;
        }
    }
    out
}

/// stem + ending with Spanish spelling rules; a few rules add a second spelling.
fn join(stem: &str, end: &str, class: Class, out: &mut HashSet<String>) {
    let first = end.chars().next();
    let gu_qu = stem.ends_with("gu") || stem.ends_with("qu");
    // Unstressed i between vowels is y: creyó, cayó, oyó, huyó, leyendo, trayendo.
    let mut e = end.to_string();
    if class != Class::Ar
        && !gu_qu
        && stem.chars().last().is_some_and(is_vowel)
        && end.starts_with('i')
        && end.chars().nth(1).is_some_and(|c| matches!(c, 'a' | 'e' | 'o'))
    {
        e = format!("y{}", &end[1..]);
    }
    let mut s = stem.to_string();
    match class {
        Class::Ar => {
            if first == Some('e') {
                if let Some(b) = s.strip_suffix('c') {
                    s = format!("{b}qu"); // busqué
                } else if s.ends_with('g') {
                    s.push('u'); // llegué
                } else if let Some(b) = s.strip_suffix('z') {
                    s = format!("{b}c"); // alcé
                }
            }
        }
        Class::Er | Class::Ir => {
            if matches!(first, Some('a') | Some('o')) {
                if let Some(b) = s.strip_suffix("gu") {
                    s = format!("{b}g"); // sigo, siga
                } else if let Some(b) = s.strip_suffix('g') {
                    s = format!("{b}j"); // escojo, escoja
                } else if let Some(b) = s.strip_suffix('c') {
                    out.insert(format!("{b}z{e}")); // venzo
                    s = format!("{b}zc"); // conozco, obedezca, aborrezca
                } else if class == Class::Ir && s.ends_with('u') && !gu_qu {
                    s.push('y'); // huyo, destruya
                }
            } else if class == Class::Ir && first == Some('e') && s.ends_with('u') && !gu_qu {
                s.push('y'); // huye, destruyen
            }
        }
    }
    out.insert(format!("{s}{e}"));
}

/// Irregular verbs and their compounds (mantener, componer, bendecir, conducir…).
struct Irregular {
    base: &'static str,
    /// Also for longer infinitives ending in `base` (the prefix is kept).
    compounds: bool,
    pret: &'static [&'static str],
    fut: Option<&'static str>,
    s1: Option<&'static str>,
    extra: &'static [&'static str],
}

const IRREGULARS: &[Irregular] = &[
    Irregular { base: "tener", compounds: true, pret: &["tuv"], fut: Some("tendr"), s1: Some("teng"), extra: &["ten"] },
    Irregular { base: "poner", compounds: true, pret: &["pus"], fut: Some("pondr"), s1: Some("pong"), extra: &["pon", "puesto", "puesta", "puestos", "puestas"] },
    Irregular { base: "venir", compounds: true, pret: &["vin"], fut: Some("vendr"), s1: Some("veng"), extra: &["ven"] },
    Irregular { base: "decir", compounds: true, pret: &["dij"], fut: Some("dir"), s1: Some("dig"), extra: &["di", "dicho", "dicha", "dichos", "dichas"] },
    Irregular { base: "bendecir", compounds: false, pret: &[], fut: None, s1: None, extra: &["bendito", "bendita", "benditos", "benditas"] },
    Irregular { base: "maldecir", compounds: false, pret: &[], fut: None, s1: None, extra: &["maldito", "maldita", "malditos", "malditas"] },
    Irregular { base: "hacer", compounds: true, pret: &["hic", "hiz"], fut: Some("har"), s1: Some("hag"), extra: &["haz", "hecho", "hecha", "hechos", "hechas"] },
    Irregular { base: "satisfacer", compounds: false, pret: &["satisfic", "satisfiz"], fut: Some("satisfar"), s1: Some("satisfag"), extra: &["satisfecho", "satisfecha", "satisfechos", "satisfechas"] },
    Irregular { base: "poder", compounds: false, pret: &["pud"], fut: Some("podr"), s1: None, extra: &[] },
    Irregular { base: "querer", compounds: false, pret: &["quis"], fut: Some("querr"), s1: None, extra: &[] },
    Irregular { base: "saber", compounds: false, pret: &["sup"], fut: Some("sabr"), s1: None, extra: &["se", "sepa", "sepas", "sepamos", "sepais", "sepan"] },
    Irregular { base: "caber", compounds: false, pret: &["cup"], fut: Some("cabr"), s1: Some("quep"), extra: &[] },
    Irregular { base: "haber", compounds: false, pret: &["hub"], fut: Some("habr"), s1: None, extra: &["he", "has", "ha", "hemos", "han", "hay", "haya", "hayas", "hayamos", "hayais", "hayan"] },
    Irregular { base: "estar", compounds: false, pret: &["estuv"], fut: None, s1: None, extra: &["estoy"] },
    Irregular { base: "andar", compounds: true, pret: &["anduv"], fut: None, s1: None, extra: &[] },
    Irregular { base: "traer", compounds: true, pret: &["traj"], fut: None, s1: Some("traig"), extra: &[] },
    Irregular { base: "caer", compounds: true, pret: &[], fut: None, s1: Some("caig"), extra: &[] },
    Irregular { base: "oir", compounds: false, pret: &[], fut: None, s1: Some("oig"), extra: &["oyes", "oye", "oyen", "oid", "oido", "oida", "oidos", "oidas", "oyendo"] },
    Irregular { base: "salir", compounds: true, pret: &[], fut: Some("saldr"), s1: Some("salg"), extra: &["sal"] },
    Irregular { base: "valer", compounds: true, pret: &[], fut: Some("valdr"), s1: Some("valg"), extra: &[] },
    Irregular { base: "ducir", compounds: true, pret: &["duj"], fut: None, s1: Some("duzc"), extra: &[] },
    Irregular { base: "morir", compounds: false, pret: &[], fut: None, s1: None, extra: &["muerto", "muerta", "muertos", "muertas"] },
    Irregular { base: "abrir", compounds: false, pret: &[], fut: None, s1: None, extra: &["abierto", "abierta", "abiertos", "abiertas"] },
    Irregular { base: "cubrir", compounds: true, pret: &[], fut: None, s1: None, extra: &["cubierto", "cubierta", "cubiertos", "cubiertas"] },
    Irregular { base: "scribir", compounds: true, pret: &[], fut: None, s1: None, extra: &["scrito", "scrita", "scritos", "scritas"] },
    Irregular { base: "olver", compounds: true, pret: &[], fut: None, s1: None, extra: &["uelto", "uelta", "ueltos", "ueltas"] },
    Irregular { base: "romper", compounds: true, pret: &[], fut: None, s1: None, extra: &["roto", "rota", "rotos", "rotas"] },
    Irregular { base: "reir", compounds: true, pret: &[], fut: None, s1: None, extra: &["rio", "rieron", "ria", "rias", "riamos", "riais", "rian", "riera", "rieran", "riese", "riere", "rieren", "riendo", "rie", "ries", "rien"] },
    Irregular {
        base: "ser",
        compounds: false,
        pret: &[],
        fut: None,
        s1: None,
        extra: &[
            "soy", "eres", "es", "somos", "sois", "son", "era", "eras", "eramos", "erais", "eran", "fui", "fuiste", "fue",
            "fuimos", "fuisteis", "fueron", "sea", "seas", "seamos", "seais", "sean", "fuera", "fueras", "fueramos",
            "fuerais", "fueran", "fuese", "fueses", "fuesen", "fuere", "fueres", "fueremos", "fuereis", "fueren", "sed",
            "siendo", "sido",
        ],
    },
    Irregular {
        base: "ir",
        compounds: false,
        pret: &[],
        fut: None,
        s1: None,
        extra: &[
            "voy", "vas", "va", "vamos", "vais", "van", "iba", "ibas", "ibamos", "ibais", "iban", "fui", "fuiste", "fue",
            "fuimos", "fuisteis", "fueron", "vaya", "vayas", "vayamos", "vayais", "vayan", "fuera", "fueras", "fueran",
            "fuese", "fuere", "fueres", "fueren", "ve", "id", "idos", "yendo", "ido",
        ],
    },
    Irregular {
        base: "dar",
        compounds: false,
        pret: &[],
        fut: None,
        s1: None,
        extra: &[
            "doy", "di", "diste", "dio", "dimos", "disteis", "dieron", "diera", "dieras", "dieramos", "dierais", "dieran",
            "diese", "diere", "dieres", "dieren", "de", "des", "demos", "deis", "den",
        ],
    },
    Irregular {
        base: "ver",
        compounds: false,
        pret: &[],
        fut: None,
        s1: None,
        extra: &[
            "veo", "vi", "viste", "vio", "vimos", "visteis", "vieron", "viera", "vieras", "vieran", "viese", "viere",
            "vieres", "vieren", "vea", "veas", "veamos", "veais", "vean", "veia", "veias", "veiamos", "veiais", "veian",
            "visto", "vista", "vistos", "vistas", "viendo",
        ],
    },
];

fn irregular_prefixes(base: &str) -> Vec<(&'static Irregular, String)> {
    IRREGULARS
        .iter()
        .filter_map(|irr| {
            if base == irr.base {
                Some((irr, String::new()))
            } else if irr.compounds && base.len() > irr.base.len() && base.ends_with(irr.base) {
                Some((irr, base[..base.len() - irr.base.len()].to_string()))
            } else {
                None
            }
        })
        .collect()
}

static FORMS: Lazy<Mutex<HashMap<String, Arc<HashSet<String>>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Every (folded) form generated for an infinitive; cached per infinitive.
pub fn forms_of(infinitive: &str) -> Arc<HashSet<String>> {
    let base = base_infinitive(infinitive);
    if let Some(f) = FORMS.lock().get(&base) {
        return f.clone();
    }
    let mut out: HashSet<String> = HashSet::new();
    if let Some((stem, class)) = split(&base) {
        out.insert(base.clone());
        if !stem.is_empty() {
            let ends = match class {
                Class::Ar => AR_END,
                Class::Er => ER_END,
                Class::Ir => IR_END,
            };
            for s in stem_variants(&stem) {
                for e in ends {
                    join(&s, e, class, &mut out);
                }
            }
        }
        for e in FUT {
            out.insert(format!("{base}{e}"));
        }
        for (irr, prefix) in irregular_prefixes(&base) {
            for p in irr.pret {
                let after_j = p.ends_with('j');
                for e in PRET_STRONG {
                    let e = if after_j { e.strip_prefix('i').filter(|r| r.starts_with('e')).unwrap_or(e) } else { e };
                    out.insert(format!("{prefix}{p}{e}"));
                }
            }
            if let Some(f) = irr.fut {
                for e in FUT {
                    out.insert(format!("{prefix}{f}{e}"));
                }
            }
            if let Some(s1) = irr.s1 {
                for e in S1_END {
                    out.insert(format!("{prefix}{s1}{e}"));
                }
            }
            for x in irr.extra {
                out.insert(format!("{prefix}{x}"));
            }
        }
    }
    let out = Arc::new(out);
    FORMS.lock().insert(base, out.clone());
    out
}

/// The word and the word without up to two attached pronouns (dámelo → dame → da;
/// levantaos → levantad; decidle → decid). The first pronoun must leave 3 letters, so
/// «Dios» is never «di» + «os».
fn word_candidates(word: &str) -> Vec<String> {
    const CLITICS: &[&str] = &["selas", "selos", "sela", "selo", "nos", "les", "los", "las", "me", "te", "se", "os", "lo", "la", "le"];
    let mut out = vec![word.to_string()];
    let mut frontier = vec![word.to_string()];
    for depth in 0..2 {
        let min_len = if depth == 0 { 3 } else { 2 };
        let mut next: Vec<String> = Vec::new();
        for x in &frontier {
            for c in CLITICS {
                if let Some(r) = x.strip_suffix(c) {
                    if r.chars().count() >= min_len {
                        next.push(r.to_string());
                        if *c == "os" && r.ends_with(['a', 'e', 'i']) {
                            next.push(format!("{r}d")); // levanta-os → levantad
                        }
                    }
                }
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

/// Is `word` a form of the verb `infinitive`? (pierda/perder, murió/morir, dijo/decir,
/// creyó/creer, arrepentíos/arrepentirse). Accents never matter.
pub fn is_form_of(word: &str, infinitive: &str) -> bool {
    if !is_infinitive(infinitive) {
        return false;
    }
    let w = fold(word);
    if w.chars().count() < 2 {
        return false;
    }
    let forms = forms_of(infinitive);
    word_candidates(&w).iter().any(|c| forms.contains(c))
}

/// Beginnings a related noun shares with this verb: the infinitive and its stems (stem
/// changes and irregular stems included), each at least 4 letters (perder → perd, pierd;
/// morir → muer; decir → dich via «dicho»).
pub fn lemma_anchors(infinitive: &str) -> Vec<String> {
    let base = base_infinitive(infinitive);
    let mut out = vec![base.clone()];
    let mut push = |s: String| {
        if s.chars().count() >= 4 && !out.contains(&s) {
            out.push(s);
        }
    };
    if let Some((stem, _)) = split(&base) {
        for v in stem_variants(&stem) {
            push(v);
        }
        for (irr, prefix) in irregular_prefixes(&base) {
            for p in irr.pret {
                push(format!("{prefix}{p}"));
            }
            for x in irr.extra {
                push(format!("{prefix}{x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_forms_of_every_tense() {
        for (w, v) in [
            ("amó", "amar"), ("amaba", "amar"), ("amaremos", "amar"), ("amare", "amar"), ("amasen", "amar"),
            ("amad", "amar"), ("amando", "amar"), ("amadas", "amar"), ("comieron", "comer"), ("comiere", "comer"),
            ("comeríais", "comer"), ("vivid", "vivir"), ("vivimos", "vivir"), ("viviesen", "vivir"),
        ] {
            assert!(is_form_of(w, v), "{w} is a form of {v}");
        }
    }

    #[test]
    fn stem_changing_verbs() {
        for (w, v) in [
            ("pierda", "perder"), ("pierden", "perder"), ("perdió", "perder"), ("murió", "morir"), ("muere", "morir"),
            ("durmió", "dormir"), ("duermen", "dormir"), ("pide", "pedir"), ("pidieron", "pedir"), ("sirve", "servir"),
            ("sintió", "sentir"), ("vuelve", "volver"), ("puede", "poder"), ("quiere", "querer"), ("juega", "jugar"),
            ("sigue", "seguir"), ("siguieron", "seguir"), ("sigo", "seguir"), ("cuenta", "contar"), ("piensa", "pensar"),
        ] {
            assert!(is_form_of(w, v), "{w} is a form of {v}");
        }
    }

    #[test]
    fn spelling_changes() {
        for (w, v) in [
            ("busqué", "buscar"), ("llegué", "llegar"), ("alcé", "alzar"), ("escoja", "escoger"), ("conozca", "conocer"),
            ("obedezcan", "obedecer"), ("aborrezco", "aborrecer"), ("venzo", "vencer"), ("creyó", "creer"),
            ("creyere", "creer"), ("leyendo", "leer"), ("cayó", "caer"), ("huyó", "huir"), ("huye", "huir"),
            ("destruyan", "destruir"), ("oyó", "oír"), ("oye", "oír"),
        ] {
            assert!(is_form_of(w, v), "{w} is a form of {v}");
        }
    }

    #[test]
    fn irregular_verbs_and_compounds() {
        for (w, v) in [
            ("dijo", "decir"), ("dijeron", "decir"), ("dirá", "decir"), ("digo", "decir"), ("dicho", "decir"),
            ("bendijo", "bendecir"), ("bendito", "bendecir"), ("predijo", "predecir"), ("hizo", "hacer"),
            ("hicieron", "hacer"), ("hecho", "hacer"), ("deshizo", "deshacer"), ("tuvo", "tener"), ("tendrá", "tener"),
            ("mantuvo", "mantener"), ("puso", "poner"), ("compuso", "componer"), ("puesto", "poner"), ("vino", "venir"),
            ("vendrá", "venir"), ("quiso", "querer"), ("supo", "saber"), ("sepa", "saber"), ("pudo", "poder"),
            ("anduvo", "andar"), ("estuvo", "estar"), ("trajo", "traer"), ("trajeron", "traer"), ("condujo", "conducir"),
            ("produzca", "producir"), ("fue", "ser"), ("fue", "ir"), ("vaya", "ir"), ("seáis", "ser"), ("dio", "dar"),
            ("vio", "ver"), ("visto", "ver"), ("muerto", "morir"), ("abierto", "abrir"), ("escrito", "escribir"),
            ("descubierto", "descubrir"), ("vuelto", "volver"), ("resuelto", "resolver"), ("salga", "salir"),
            ("hubo", "haber"), ("hay", "haber"), ("cupo", "caber"),
        ] {
            assert!(is_form_of(w, v), "{w} is a form of {v}");
        }
    }

    #[test]
    fn attached_pronouns_and_reflexive_glosses() {
        for (w, v) in [
            ("arrepentíos", "arrepentirse"), ("levantaos", "levantar"), ("dámelo", "dar"), ("decidle", "decir"),
            ("amarle", "amar"), ("diciéndoles", "decir"), ("bautícese", "bautizar"),
        ] {
            assert!(is_form_of(w, v), "{w} is a form of {v}");
        }
    }

    #[test]
    fn not_forms() {
        for (w, v) in [
            ("pierda", "perecer"), ("vino", "ver"), ("dijo", "dar"), ("mar", "amar"), ("amor", "amar"),
            ("perdición", "perder"), ("muerte", "morir"), ("hombre", "hacer"), ("dios", "decir"), ("pierda", "lugar"),
        ] {
            assert!(!is_form_of(w, v), "{w} is not a form of {v}");
        }
        assert!(!is_form_of("amó", "amor"), "not an infinitive");
        assert!(!is_form_of("", "amar"));
    }

    #[test]
    fn anchors_carry_stem_changes_and_irregular_stems() {
        let a = lemma_anchors("perder");
        assert!(a.contains(&"perder".to_string()) && a.contains(&"perd".to_string()) && a.contains(&"pierd".to_string()), "{a:?}");
        assert!(lemma_anchors("morir").contains(&"muer".to_string()));
        assert!(lemma_anchors("decir").contains(&"dicho".to_string()));
        assert!(lemma_anchors("arrepentirse").contains(&"arrepentir".to_string()));
        assert!(lemma_anchors("ver").iter().all(|s| s == "ver" || s.chars().count() >= 4));
    }
}
