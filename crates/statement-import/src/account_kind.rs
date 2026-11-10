//! Guessing a French account type (livret, PEL, compte courant...) from the label printed on a document.

use chrono::NaiveDate;

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

const KEYWORDS: &[(&str, &[&str])] = &[
    ("livret_a", &["livret a", "livret bleu"]),
    (
        "ldds",
        &[
            "ldds",
            "ldd ",
            "developpement durable",
            "développement durable",
        ],
    ),
    ("lep", &["lep", "epargne populaire", "épargne populaire"]),
    ("livret_jeune", &["livret jeune"]),
    (
        "pel",
        &[
            "pel",
            "plan epargne logement",
            "plan d'épargne logement",
            "plan epargne-logement",
        ],
    ),
    (
        "cel",
        &[
            "cel",
            "compte epargne logement",
            "compte d'épargne logement",
        ],
    ),
    ("pea", &["pea"]),
    (
        "per",
        &["per ", "plan epargne retraite", "plan d'épargne retraite"],
    ),
    (
        "life_insurance",
        &["assurance vie", "assurance-vie", "contrat vie"],
    ),
    (
        "cto",
        &["compte titres", "compte-titres", "cto", "ord. titres"],
    ),
    (
        "term_deposit",
        &["compte a terme", "compte à terme", "cat "],
    ),
    (
        "mortgage",
        &["pret immo", "prêt immo", "credit immo", "crédit immo"],
    ),
    ("loan", &["pret ", "prêt ", "credit ", "crédit "]),
    (
        "savings",
        &["livret", "epargne", "épargne", "compte sur livret", "csl"],
    ),
    (
        "checking",
        &[
            "compte courant",
            "compte de depot",
            "compte de dépôt",
            "compte cheque",
            "compte chèque",
        ],
    ),
];

/// Python's `\w` for one character.
pub fn is_word(c: char) -> bool {
    c == '_' || crate::py::is_alnum_char(c)
}

/// `re.search(r"(?<!\w)" + re.escape(word) + r"(?!\w)", text)`.
pub fn contains_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return true;
    }
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let at = start + pos;
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        if !before.map(is_word).unwrap_or(false) && !after.map(is_word).unwrap_or(false) {
            return true;
        }
        start = at + text[at..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    false
}

pub fn guess_account_type(label: &str, default: &str) -> String {
    let text = label.to_lowercase();
    for (code, words) in KEYWORDS {
        for w in *words {
            if contains_word(&text, crate::py::strip(w)) {
                return code.to_string();
            }
        }
    }
    default.to_string()
}
