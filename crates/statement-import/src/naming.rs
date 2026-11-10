//! The standard name a stored document is downloaded under:
//! `INSTITUTION_ACCOUNT-TYPE_DOCUMENT-TYPE_HOLDER_YYYY-MM-DD.ext` (accent-free, upper case; words and the date are
//! joined with `-`, fields with `_`).

use chrono::NaiveDate;

use crate::adapters::common::strip_accents;

const MAX_NAME_CHARS: usize = 150;

/// A word list of a field: accents removed, upper case, anything but letters and digits is a separator.
fn words(text: &str) -> Vec<String> {
    strip_accents(text)
        .to_uppercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn field(text: &str) -> String {
    words(text).join("-")
}

/// The account type as a French label (the reader's own taxonomy in, a label out).
pub fn account_type_label(account_type: Option<&str>) -> &'static str {
    match account_type.unwrap_or("") {
        "checking" => "Compte courant",
        "savings" => "Livret",
        "livret_a" => "Livret A",
        "ldds" => "LDDS",
        "lep" => "LEP",
        "livret_jeune" => "Livret Jeune",
        "pel" => "PEL",
        "cel" => "CEL",
        "term_deposit" => "Compte à terme",
        "life_insurance" => "Assurance-vie",
        "mortgage" | "loan" => "Crédit",
        "pea" => "PEA",
        "cto" => "Compte-titres",
        "crypto_exchange" => "Crypto",
        _ => "Compte",
    }
}

/// The kind of document as a French label.
pub fn document_kind_label(kind: Option<&str>) -> String {
    match kind.unwrap_or("") {
        "statement" | "ofx" | "qif" | "csv" | "pdf" => "Relevé de compte".into(),
        "savings_statement" => "Relevé d'épargne".into(),
        "fees_summary" => "Récapitulatif des frais".into(),
        "loan_offer" => "Offre de prêt".into(),
        "loan_schedule" => "Tableau d'amortissement".into(),
        "loan_event" => "Remboursement anticipé".into(),
        "info" => "Courrier".into(),
        "" => "Document".into(),
        other => other.replace('_', " "),
    }
}

/// `extension` is the file's own (with or without the dot); the name is cut by dropping the first names, then the holder,
/// when it would be too long.
pub fn standard_file_name(
    institution: Option<&str>,
    account_type: Option<&str>,
    kind: Option<&str>,
    holder: Option<&str>,
    date: Option<NaiveDate>,
    extension: &str,
) -> String {
    let ext = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let ext = if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        "bin".to_string()
    } else {
        ext
    };
    let institution = institution
        .map(field)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "DOCUMENT".into());
    let account = field(account_type_label(account_type));
    let kind = field(&document_kind_label(kind));
    let date = date.map(|d| d.format("%Y-%m-%d").to_string());

    let holder_words = holder.map(words).unwrap_or_default();
    let build = |holder: &[String]| {
        let mut fields = vec![institution.clone(), account.clone(), kind.clone()];
        if !holder.is_empty() {
            fields.push(holder.join("-"));
        }
        if let Some(d) = &date {
            fields.push(d.clone());
        }
        format!("{}.{}", fields.join("_"), ext)
    };
    let name = build(&holder_words);
    if name.chars().count() <= MAX_NAME_CHARS {
        return name;
    }
    // too long: keep the surname (the first word) and drop the rest, then the holder altogether
    let name = build(&holder_words[..1.min(holder_words.len())]);
    if name.chars().count() <= MAX_NAME_CHARS {
        return name;
    }
    build(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(2026, 9, 30)
    }

    #[test]
    fn the_name_is_accent_free_upper_case_with_fields_split_by_underscores() {
        let name = standard_file_name(
            Some("BoursoBank"),
            Some("checking"),
            Some("statement"),
            Some("DUPONT Jean-Éric"),
            day(),
            "PDF",
        );
        assert_eq!(
            name,
            "BOURSOBANK_COMPTE-COURANT_RELEVE-DE-COMPTE_DUPONT-JEAN-ERIC_2026-09-30.pdf"
        );
    }

    #[test]
    fn a_document_without_holder_or_date_still_has_a_name() {
        let name = standard_file_name(
            Some("Fortuneo"),
            None,
            Some("fees_summary"),
            None,
            None,
            ".pdf",
        );
        assert_eq!(name, "FORTUNEO_COMPTE_RECAPITULATIF-DES-FRAIS.pdf");
    }

    #[test]
    fn a_too_long_name_loses_the_first_names_then_the_holder() {
        let long = (0..40)
            .map(|i| format!("PRENOM{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let name = standard_file_name(
            Some("Fortuneo"),
            Some("checking"),
            Some("statement"),
            Some(&format!("DUPONT {long}")),
            day(),
            "pdf",
        );
        assert_eq!(
            name,
            "FORTUNEO_COMPTE-COURANT_RELEVE-DE-COMPTE_DUPONT_2026-09-30.pdf"
        );
    }

    #[test]
    fn an_odd_extension_falls_back_to_bin() {
        let name = standard_file_name(None, None, None, None, None, "a/b");
        assert_eq!(name, "DOCUMENT_COMPTE_DOCUMENT.bin");
    }
}
