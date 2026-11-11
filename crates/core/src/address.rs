//! Postal addresses kept as one text ("street\npostal code city") and read or written as street, postal code and city.
//! No migration is needed: the stored text is the same one the statements print.

use serde::{Deserialize, Serialize};

/// An address cut into the parts a form shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddressParts {
    pub street: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
}

fn non_empty(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty()).then(|| v.to_string())
}

fn is_postal_code(token: &str) -> bool {
    (4..=6).contains(&token.len()) && token.chars().all(|c| c.is_ascii_digit())
}

/// Cut an address into street, postal code and city. The postal code is the first group of 4 to 6 digits of the last
/// line that is followed by the city; everything before it, and the lines above, make the street.
pub fn split_address(address: &str) -> AddressParts {
    let mut lines: Vec<&str> = address
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let Some(last) = lines.pop() else {
        return AddressParts::default();
    };
    let tokens: Vec<&str> = last.split_whitespace().collect();
    let found = tokens
        .iter()
        .enumerate()
        .position(|(i, t)| is_postal_code(t) && i + 1 < tokens.len());
    let (mut street_lines, postal_code, city) = match found {
        Some(i) => {
            let mut street: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            if i > 0 {
                street.push(tokens[..i].join(" "));
            }
            (
                street,
                Some(tokens[i].to_string()),
                non_empty(&tokens[i + 1..].join(" ")),
            )
        }
        None => {
            let mut street: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            street.push(last.to_string());
            (street, None, None)
        }
    };
    street_lines.retain(|l| !l.is_empty());
    AddressParts {
        street: non_empty(&street_lines.join(", ")),
        postal_code,
        city,
    }
}

/// Put the parts back into the stored text; `None` when there is nothing.
pub fn join_address(parts: &AddressParts) -> Option<String> {
    let town = [parts.postal_code.as_deref(), parts.city.as_deref()]
        .iter()
        .flatten()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let lines: Vec<String> = [parts.street.as_deref().map(str::trim).unwrap_or(""), &town]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The address to store when a caller gives either the whole text or some of the parts. A part that is given
/// replaces the same part of `existing` (an empty one clears it); parts that are not given stay as they were. With no
/// part given the whole text is used as it is.
pub fn resolve_address(
    existing: Option<&str>,
    whole: &Option<String>,
    street: &Option<String>,
    postal_code: &Option<String>,
    city: &Option<String>,
) -> Option<Option<String>> {
    if street.is_none() && postal_code.is_none() && city.is_none() {
        return whole.as_ref().map(|w| non_empty(w));
    }
    let mut parts = split_address(existing.unwrap_or(""));
    if let Some(v) = street {
        parts.street = non_empty(v);
    }
    if let Some(v) = postal_code {
        parts.postal_code = non_empty(v);
    }
    if let Some(v) = city {
        parts.city = non_empty(v);
    }
    Some(join_address(&parts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_printed_address() {
        let p = split_address("45 RUE ALEXANDRE BICKART\n77500 CHELLES");
        assert_eq!(p.street.as_deref(), Some("45 RUE ALEXANDRE BICKART"));
        assert_eq!(p.postal_code.as_deref(), Some("77500"));
        assert_eq!(p.city.as_deref(), Some("CHELLES"));
    }

    #[test]
    fn splits_a_single_line_and_a_missing_town() {
        let p = split_address("1 RUE DU TEST 60200 COMPIEGNE");
        assert_eq!(p.street.as_deref(), Some("1 RUE DU TEST"));
        assert_eq!(p.postal_code.as_deref(), Some("60200"));
        assert_eq!(p.city.as_deref(), Some("COMPIEGNE"));
        let q = split_address("Chez Martin\n12 rue des Lilas");
        assert_eq!(q.street.as_deref(), Some("Chez Martin, 12 rue des Lilas"));
        assert_eq!(q.postal_code, None);
        assert_eq!(split_address("  "), AddressParts::default());
    }

    #[test]
    fn joins_and_merges_parts() {
        let parts = AddressParts {
            street: Some("45 RUE X".into()),
            postal_code: Some("77500".into()),
            city: Some("CHELLES".into()),
        };
        assert_eq!(
            join_address(&parts).as_deref(),
            Some("45 RUE X\n77500 CHELLES")
        );
        let merged = resolve_address(
            Some("45 RUE X\n77500 CHELLES"),
            &None,
            &None,
            &Some("60200".into()),
            &Some("COMPIEGNE".into()),
        );
        assert_eq!(merged, Some(Some("45 RUE X\n60200 COMPIEGNE".to_string())));
        assert_eq!(
            resolve_address(Some("a"), &Some(" ".into()), &None, &None, &None),
            Some(None)
        );
        assert_eq!(resolve_address(Some("a"), &None, &None, &None, &None), None);
    }
}
