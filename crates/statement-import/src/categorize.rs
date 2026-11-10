//! Suggests a spending or income category for each operation of a statement, and says how sure it is.
//!
//! The suggestion is read from the label the bank printed (merchant, counterparty, memo), the kind of operation
//! the reader recognised (card, direct debit, transfer, fee...), the sign and the account holder's name (a transfer
//! to or from oneself is not an expense or an income). Nothing here talks to a database: the categories are named
//! by the ids Wealthfolio seeds (`cat_<key>`), and the caller decides what to do with the suggestion.
//!
//! Confidence tells the review screen where to look:
//! - `high`: a known merchant or an unambiguous label;
//! - `medium`: a good guess from a keyword or from the kind of operation, shown in the review without insisting;
//! - `low`: a guess that is little better than "other", or something that may not be what it seems: these are the
//!   operations a person is asked to look at.

use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock as Lazy;

pub const SPENDING_TAXONOMY: &str = "spending_categories";
pub const INCOME_TAXONOMY: &str = "income_sources";

pub const HIGH: &str = "high";
pub const MEDIUM: &str = "medium";
pub const LOW: &str = "low";

/// A category proposed for one operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySuggestion {
    pub taxonomy_id: String,
    pub category_id: String,
    /// `high`, `medium` or `low`.
    pub confidence: String,
    /// Why it is doubtful (or what it was recognised as when it is not).
    pub reason: Option<String>,
}

impl CategorySuggestion {
    /// Only a low-confidence guess is worth stopping for: a medium one is a good guess that the review shows
    /// without insisting.
    pub fn needs_review(&self) -> bool {
        self.confidence == LOW
    }
}

struct Rule {
    pattern: Regex,
    category: &'static str,
    confidence: &'static str,
    note: Option<&'static str>,
}

fn rule(
    pattern: &str,
    category: &'static str,
    confidence: &'static str,
    note: Option<&'static str>,
) -> Rule {
    Rule {
        pattern: Regex::new(pattern).expect("category rule"),
        category,
        confidence,
        note,
    }
}

// Outgoing money. First match wins, so the specific merchants come before the generic keywords.
static EXPENSE_RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    vec![
        // the transfer a bank asks to open an account: money moved from or to the person's other account
        rule(
            r"(VIREMENT CREATION COMPTE|VIREMENT D'?OUVERTURE|VIREMENT DE CREATION)",
            "transfers_own",
            MEDIUM,
            Some("Virement d'ouverture de compte (compte à compte)"),
        ),
        // money that leaves the account for another account of the person (crypto exchange, broker): a transfer
        // between own accounts, not something consumed
        rule(
            r"\b(KRAKEN|COINBASE|BINANCE|BITSTAMP|BITPANDA|LEDGER LIVE|CRYPTO\.COM)\b",
            "transfers_crypto",
            MEDIUM,
            Some("Virement vers un compte crypto (compte à compte)"),
        ),
        rule(
            r"\b(TRADE REPUBLIC|DEGIRO|INTERACTIVE BROKERS|BOURSORAMA PLACEMENT)\b",
            "transfers_broker",
            MEDIUM,
            Some("Virement vers un compte-titres ou un courtier (compte à compte)"),
        ),
        rule(
            r"\b(UBER ?EATS|DELIVEROO|JUST ?EAT)\b",
            "food_delivery",
            HIGH,
            None,
        ),
        rule(
            r"\b(CARREFOUR|LECLERC|AUCHAN|LIDL|ALDI|INTERMARCHE|MONOPRIX|FRANPRIX|PICARD|SUPER ?U|NETTO|BIOCOOP|CASINO SHOP|G20|COLRUYT)\b",
            "groceries",
            HIGH,
            None,
        ),
        rule(
            r"\b(MC ?DONALD'?S?|MCDO|BURGER KING|KFC|QUICK|SUBWAY|DOMINO'?S|PIZZA ?HUT|STARBUCKS|PAUL|BRIOCHE DOREE|FLUNCH|BUFFALO GRILL|HIPPOPOTAMUS)\b",
            "food_restaurants",
            HIGH,
            None,
        ),
        rule(
            r"(RESTAURANT|RESTO|BRASSERIE|PIZZ|SUSHI|BOULANGERIE|PATISSERIE|BOUCHERIE|CAFE |BISTRO|KEBAB|TACOS)",
            "food_restaurants",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(UBER|BOLT|HEETCH|FREE ?NOW|TAXI|G7)\b",
            "transport_rideshare",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(SNCF|RATP|NAVIGO|TRANSILIEN|OUIGO|FLIXBUS|BLABLACAR|TCL|RTM|KEOLIS|TISSEO|IDFM|ILEVIA)\b",
            "transport_public",
            HIGH,
            None,
        ),
        rule(
            r"\b(TOTAL ?ENERGIES? ACCESS|TOTAL ACCESS|ESSO|SHELL|BP |AVIA|STATION|CARBURANT|INTERMARCHE CARBURANT)\b",
            "transport_gas",
            MEDIUM,
            None,
        ),
        rule(
            r"(PARKING|INDIGO|Q-?PARK|SAEMES|EFFIA)",
            "transport_parking",
            HIGH,
            None,
        ),
        rule(
            r"(TELEPEAGE|AUTOROUTE|VINCI AUTOROUTES|SANEF|APRR|COFIROUTE)",
            "transport",
            HIGH,
            None,
        ),
        rule(
            r"\b(NETFLIX|SPOTIFY|DISNEY ?PLUS|DISNEY\+|DEEZER|CANAL ?\+?|PRIME VIDEO|MOLOTOV|CRUNCHYROLL|APPLE MUSIC|TIDAL|OCS)\b",
            "entertainment_streaming",
            HIGH,
            None,
        ),
        rule(
            r"(YOUTUBE|GOOGLE YO)",
            "entertainment_streaming",
            MEDIUM,
            Some("Abonnement YouTube / Google : musique, vidéo ou stockage ?"),
        ),
        rule(
            r"\b(STEAM|PLAYSTATI(ON)?|XBOX|NINTENDO|EPIC GAMES|BLIZZARD|BATTLE\.NET|UBISOFT|ORIGIN|RIOT GAMES|GOG\.COM|HUMBLE BUNDLE)",
            "entertainment_games",
            HIGH,
            None,
        ),
        rule(
            r"\b(SHADOW|GEFORCE NOW|PATREON|TWITCH|ONLYFANS|TIPEEE|UTIP)",
            "bills_subscriptions",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(MICROSOFT|ADOBE|DROPBOX|OVH|GITHUB|OPENAI|NOTION|ICLOUD|APPLE\.COM/BILL|GOOGLE ?(ONE|CLOUD|WORKSPACE)|AWS|SCALEWAY|NAMECHEAP|GANDI|1PASSWORD|LASTPASS|PROTON)",
            "bills_software",
            MEDIUM,
            Some("Service en ligne : logiciel ou abonnement ?"),
        ),
        rule(
            r"\b(ORANGE|SFR|BOUYGUES|FREE MOBILE|SOSH|RED BY SFR|B&YOU|LEBARA|PRIXTEL|LA POSTE MOBILE|NRJ MOBILE|CORIOLIS)\b",
            "bills_phone",
            HIGH,
            None,
        ),
        rule(
            r"\b(FREEBOX|LIVEBOX|BBOX|SFR BOX|FREE TELECOM|FREE HAUT DEBIT)\b",
            "bills_internet",
            HIGH,
            None,
        ),
        rule(
            r"\b(EDF|ENGIE|ENI|VEOLIA|SUEZ|ENEDIS|GRDF|OHM ENERGIE|MINT ENERGIE)\b",
            "housing_utilities",
            HIGH,
            None,
        ),
        rule(
            r"(LOYER|AGENCE IMMO|FONCIA|NEXITY|CENTURY 21|ORPI|SERGIC|SQUARE HABITAT)",
            "housing_rent",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(MAIF|MACIF|MMA|AXA|GROUPAMA|MATMUT|ALLIANZ|GENERALI|GMF|AVIVA|LEOCARE|LEMONADE|ASSURANCE)\b",
            "housing_insurance",
            LOW,
            Some("Assurance : habitation, auto ou autre ?"),
        ),
        rule(
            r"\b(PHARMACIE|PHARMA|PARAPHARMACIE)\b",
            "health_pharmacy",
            HIGH,
            None,
        ),
        rule(
            r"\b(DOCTOLIB|MEDECIN|DOCTEUR|HOPITAL|CLINIQUE|LABORATOIRE|RADIOLOG|KINE|OSTEO|CPAM)\b",
            "health_medical",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(DENTISTE|DENTAIRE|ORTHODONT)",
            "health_dental",
            HIGH,
            None,
        ),
        rule(
            r"\b(OPTIQUE|OPTICIEN|KRYS|AFFLELOU|GRAND OPTICAL|ALAIN AFFLELOU|GENERALE D'OPTIQUE|LUNETTES)\b",
            "health_vision",
            HIGH,
            None,
        ),
        rule(
            r"\b(BASIC ?FIT|FITNESS|NEONESS|KEEP COOL|ON AIR|GYM|SALLE DE SPORT|CROSSFIT)\b",
            "health_fitness",
            HIGH,
            None,
        ),
        rule(
            r"\b(MUTUELLE|HARMONIE|MGEN|MALAKOFF|ALAN|APRIL SANTE)\b",
            "health_insurance",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(FNAC|BOULANGER|DARTY|LDLC|MATERIEL\.NET|APPLE STORE|MICROSOFT STORE|LEDGER|RHINOSHIELD|BACK ?MARKET|ONE ?PLUS|SAMSUNG|XIAOMI|ELECTRO ?DEPOT)",
            "shopping_electronics",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(IKEA|LEROY ?MERLIN|CASTORAMA|BRICO|MAISONS DU MONDE|CONFORAMA|BUT |ALINEA|BRICORAMA|MR BRICOLAGE|GIFI)\b",
            "shopping_home",
            HIGH,
            None,
        ),
        rule(
            r"\b(ZARA|H ?& ?M|KIABI|CELIO|UNIQLO|DECATHLON|JULES|PIMKIE|PRIMARK|SHEIN|SPORT ?2000|INTERSPORT)\b",
            "shopping_clothing",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(AMAZON|AMZN|CDISCOUNT|ALIEXPRESS|EBAY|VINTED|ZALANDO|WISH|TEMU|ETSY|RAKUTEN|PRICEMINISTER|LEBONCOIN)",
            "shopping_online",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(AIRBNB|BOOKING|HOTEL|AIR FRANCE|RYANAIR|EASYJET|EXPEDIA|TRAINLINE|SKYSCANNER|TRANSAVIA|VUELING|LUFTHANSA|KLM|OUI\.SNCF|IBIS|ACCOR|CAMPING|LOCATION VOITURE|HERTZ|AVIS|EUROPCAR)",
            "travel",
            HIGH,
            None,
        ),
        rule(
            r"(CINEMA|UGC|PATHE|GAUMONT|MK2|TICKETMASTER|FNAC SPECTACLES|BILLETTERIE|CONCERT|THEATRE|MUSEE|PARC )",
            "entertainment_movies",
            MEDIUM,
            None,
        ),
        rule(
            r"(UDEMY|COURSERA|OPENCLASSROOMS|UNIVERSITE|ECOLE|FORMATION|COURS )",
            "education",
            MEDIUM,
            None,
        ),
        rule(
            r"(COIFFEUR|COIFFURE|SEPHORA|NOCIBE|BARBER|ESTHETI|INSTITUT DE BEAUTE|YVES ROCHER|MARIONNAUD)",
            "personal",
            MEDIUM,
            None,
        ),
        rule(
            r"(CADEAU|ANNIVERSAIRE|JOYEUX NOEL|NAISSANCE|MARIAGE)",
            "gifts",
            MEDIUM,
            None,
        ),
        rule(
            r"\b(WIKIMEDIA|MEDECINS SANS|UNICEF|CROIX ROUGE|RESTOS DU COEUR|FONDATION|DON )",
            "gifts",
            MEDIUM,
            Some("Don ou cotisation ?"),
        ),
        rule(r"(AMENDE|ANTAI)", "taxes_fines", HIGH, None),
        rule(
            r"(TAXE FONCIERE|TAXE D'?HABITATION)",
            "taxes_property",
            HIGH,
            None,
        ),
        rule(
            r"(IMPOT|DGFIP|TRESOR PUBLIC|DIRECTION GENERALE DES FINANCES|URSSAF|TAXE)",
            "taxes",
            MEDIUM,
            Some("Impôt ou taxe"),
        ),
        rule(
            r"(RETRAIT|DAB |DISTRIBUTEUR|CASH ?WITHDRAWAL|ATM )",
            "cash_withdrawal",
            MEDIUM,
            Some("Retrait d'espèces"),
        ),
        rule(
            r"(COTISATION|FRAIS|COMMISSION|AGIOS|TENUE DE COMPTE|CARTE BANCAIRE|CB VISA)",
            "fees_bank",
            MEDIUM,
            None,
        ),
        rule(
            r"(INTERETS DEBITEURS|INTERET DEBITEUR)",
            "fees_interest",
            HIGH,
            None,
        ),
    ]
});

// Incoming money.
static INCOME_RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    vec![
        // the transfer a bank asks to open an account: money moved from or to the person's other account
        rule(
            r"(VIREMENT CREATION COMPTE|VIREMENT D'?OUVERTURE|VIREMENT DE CREATION)",
            "income_transfers_own",
            MEDIUM,
            Some("Virement d'ouverture de compte (compte à compte)"),
        ),
        rule(
            r"\b(KRAKEN|COINBASE|BINANCE|BITSTAMP|BITPANDA|LEDGER LIVE|CRYPTO\.COM)\b",
            "income_transfers_crypto",
            MEDIUM,
            Some("Virement depuis un compte crypto (compte à compte)"),
        ),
        rule(
            r"\b(TRADE REPUBLIC|DEGIRO|INTERACTIVE BROKERS|BOURSORAMA PLACEMENT)\b",
            "income_transfers_broker",
            MEDIUM,
            Some("Virement depuis un compte-titres ou un courtier (compte à compte)"),
        ),
        rule(
            r"(SALAIRE|PAIE |PAYE |PAYROLL|REMUNERATION|TRAITEMENT)",
            "income_salary",
            HIGH,
            None,
        ),
        rule(
            r"(PRIME BRS|OFFRE MOBILITE|MOBILITE BANCAIRE|PARRAINAGE|PRIME DE BIENVENUE|BIENVENUE|PRIME )",
            "income_prime",
            MEDIUM,
            Some("Prime ou offre d'une banque"),
        ),
        rule(r"(DIVIDENDE|COUPON)", "income_dividends", HIGH, None),
        rule(
            r"\b(INTERETS?|INTERET CREDITEUR|INTERETS CREDITEURS)\b",
            "income_interest",
            HIGH,
            None,
        ),
        rule(r"(LOYER PERCU|LOYERS)", "income_rental", MEDIUM, None),
        rule(
            r"(REMBOURSEMENT TRESOR|DGFIP|IMPOT)",
            "income_tax_refund",
            MEDIUM,
            None,
        ),
        rule(
            r"(CPAM|AMELI|MUTUELLE|HARMONIE|MGEN|ALAN)",
            "income_reimbursements",
            HIGH,
            None,
        ),
        rule(
            r"(REMBOURSEMENT|REMB |RBT|REMBT|AVOIR)",
            "income_reimbursements",
            MEDIUM,
            None,
        ),
        rule(
            r"(CAF |POLE EMPLOI|FRANCE TRAVAIL|ALLOCATION|ARE )",
            "income_benefits",
            MEDIUM,
            Some("Aide ou allocation"),
        ),
        rule(
            r"(CADEAU|ANNIVERSAIRE|JOYEUX NOEL|NAISSANCE|ETRENNES)",
            "income_gifts",
            MEDIUM,
            None,
        ),
        rule(
            r"(FREELANCE|HONORAIRES|FACTURE)",
            "income_freelance",
            MEDIUM,
            None,
        ),
    ]
});

static FOREIGN_CURRENCY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b\d+[.,]?\d*\s?(USD|GBP|CHF|CAD|JPY|AUD|SEK|NOK|DKK|PLN)\b").unwrap()
});

const TITLES: &[&str] = &[
    "M",
    "MR",
    "MME",
    "MLE",
    "MLLE",
    "MONSIEUR",
    "MADAME",
    "MADEMOISELLE",
    "OU",
    "ET",
    "DE",
    "DU",
    "LA",
    "LE",
];

fn tokens(text: &str) -> Vec<String> {
    text.to_uppercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| {
            t.trim_start_matches(|c: char| c.is_ascii_digit())
                .to_string()
        })
        .filter(|t| t.len() > 1 && !TITLES.contains(&t.as_str()))
        .collect()
}

/// How far a counterparty looks like the account holder: `Full` for the same name, `Partial` for a name that is
/// part of (or a truncation of) the holder's.
#[derive(Debug, PartialEq, Eq)]
enum SelfMatch {
    Full,
    Partial,
    No,
}

fn self_match(holder: Option<&str>, counterparty: &str) -> SelfMatch {
    let Some(holder) = holder else {
        return SelfMatch::No;
    };
    let holder_tokens = tokens(holder);
    let other = tokens(counterparty);
    if holder_tokens.is_empty() || other.is_empty() {
        return SelfMatch::No;
    }
    let matches = |t: &String| {
        holder_tokens
            .iter()
            .any(|h| h == t || (t.len() >= 5 && h.starts_with(t.as_str())))
    };
    if !other.iter().all(matches) {
        return SelfMatch::No;
    }
    let every_holder_token_present = holder_tokens.iter().all(|h| {
        other
            .iter()
            .any(|t| t == h || (t.len() >= 5 && h.starts_with(t.as_str())))
    });
    if every_holder_token_present {
        SelfMatch::Full
    } else {
        SelfMatch::Partial
    }
}

fn suggestion(
    taxonomy: &str,
    key: &str,
    confidence: &str,
    reason: Option<String>,
) -> CategorySuggestion {
    CategorySuggestion {
        taxonomy_id: taxonomy.to_string(),
        category_id: format!("cat_{key}"),
        confidence: confidence.to_string(),
        reason,
    }
}

/// Everything the suggestion reads about one operation.
pub struct CategoryInput<'a> {
    pub label: &'a str,
    pub counterparty: Option<&'a str>,
    pub memo: Option<&'a str>,
    /// What the reader recognised: card, direct_debit, transfer, instant_transfer, fee, interest, loan_payment...
    pub kind: Option<&'a str>,
    /// Signed: negative for money going out.
    pub amount: f64,
    pub holder_name: Option<&'a str>,
}

fn worse(a: &'static str, b: &'static str) -> &'static str {
    let rank = |c: &str| match c {
        HIGH => 2,
        MEDIUM => 1,
        _ => 0,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// The category proposed for one operation.
pub fn suggest(input: &CategoryInput) -> CategorySuggestion {
    let outgoing = input.amount < 0.0;
    let taxonomy = if outgoing {
        SPENDING_TAXONOMY
    } else {
        INCOME_TAXONOMY
    };
    let haystack = format!(
        "{} {} {}",
        input.label,
        input.counterparty.unwrap_or(""),
        input.memo.unwrap_or("")
    )
    .to_uppercase();
    let kind = input.kind.unwrap_or("");
    let abs = input.amount.abs();

    // fees and interest are told by the reader itself
    if kind == "fee" && outgoing {
        return suggestion(taxonomy, "fees_bank", HIGH, None);
    }
    if kind == "interest" {
        return if outgoing {
            suggestion(taxonomy, "fees_interest", HIGH, None)
        } else {
            suggestion(taxonomy, "income_interest", HIGH, None)
        };
    }
    if kind == "loan_payment" && outgoing {
        return suggestion(
            taxonomy,
            "housing_loan",
            MEDIUM,
            Some("Échéance de prêt : logement, auto ou conso ?".into()),
        );
    }

    // an explicit label wins over the "it is a transfer to myself" reading ("1FERRAN — Remboursement one plus")
    let rules: &[Rule] = if outgoing {
        &EXPENSE_RULES
    } else {
        &INCOME_RULES
    };
    let counterparty = input.counterparty.unwrap_or("");
    let own = self_match(input.holder_name, counterparty);
    let is_self_transfer = matches!(kind, "transfer" | "instant_transfer" | "other" | "")
        && own != SelfMatch::No
        && !counterparty.is_empty();

    // for a transfer that carries the holder's own name only what was written next to it can say what it is for
    let text = if is_self_transfer {
        format!(
            "{} {}",
            input
                .label
                .to_uppercase()
                .replace(&counterparty.to_uppercase(), " "),
            input.memo.unwrap_or("").to_uppercase()
        )
    } else {
        haystack.clone()
    };
    if let Some(r) = rules.iter().find(|r| r.pattern.is_match(&text)) {
        let mut confidence = r.confidence;
        let mut reasons: Vec<String> = r.note.map(String::from).into_iter().collect();
        if outgoing && FOREIGN_CURRENCY.is_match(&haystack) {
            confidence = worse(confidence, MEDIUM);
            reasons.push("Payé en devise étrangère".into());
        }
        if is_self_transfer {
            confidence = worse(confidence, MEDIUM);
            reasons.push("Virement vers ou depuis votre propre nom".into());
        }
        return suggestion(
            taxonomy,
            r.category,
            confidence,
            if reasons.is_empty() {
                None
            } else {
                Some(reasons.join(" · "))
            },
        );
    }

    if is_self_transfer {
        // the holder's own name is a transfer between own accounts: sure when the whole name is there, to be
        // looked at when only a part of it (or a name that looks like it) is
        let (reason, confidence) = match (outgoing, own) {
            (true, SelfMatch::Full) => (
                "Virement vers vous-même : compte à compte, ce n'est pas une dépense",
                MEDIUM,
            ),
            (false, SelfMatch::Full) => (
                "Virement de vous-même : compte à compte, ce n'est pas un revenu",
                MEDIUM,
            ),
            (true, _) => (
                "Le nom ressemble au vôtre : virement vers un de vos comptes ?",
                LOW,
            ),
            (false, _) => (
                "Le nom ressemble au vôtre : virement depuis un de vos comptes ?",
                LOW,
            ),
        };
        return suggestion(
            taxonomy,
            if outgoing {
                "transfers_own"
            } else {
                "income_transfers_own"
            },
            confidence,
            Some(reason.to_string()),
        );
    }

    // nothing recognised: a guess from the kind of operation and the amount
    if outgoing {
        match kind {
            "card" => {
                let mut reasons = vec!["Commerçant non reconnu".to_string()];
                if FOREIGN_CURRENCY.is_match(&haystack) {
                    reasons.push("payé en devise étrangère".into());
                }
                if abs >= 50.0 && abs.fract() == 0.0 && abs % 10.0 == 0.0 {
                    reasons.push("montant rond : retrait d'espèces ou achat ?".into());
                }
                suggestion(taxonomy, "shopping", LOW, Some(reasons.join(" · ")))
            }
            "direct_debit" => suggestion(
                taxonomy,
                "bills_subscriptions",
                LOW,
                Some("Prélèvement non reconnu".into()),
            ),
            "transfer" | "instant_transfer" => suggestion(
                taxonomy,
                "other_expense",
                LOW,
                Some("Virement vers un tiers : à qui et pour quoi ?".into()),
            ),
            _ => suggestion(
                taxonomy,
                "other_expense",
                LOW,
                Some("Opération non reconnue".into()),
            ),
        }
    } else {
        match kind {
            "transfer" | "instant_transfer" => suggestion(
                taxonomy,
                "income_other",
                LOW,
                Some("Virement reçu d'un tiers : à qui et pour quoi ?".into()),
            ),
            _ => suggestion(
                taxonomy,
                "income_other",
                LOW,
                Some("Entrée d'argent non reconnue".into()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        label: &'a str,
        cp: Option<&'a str>,
        kind: &'a str,
        amount: f64,
    ) -> CategoryInput<'a> {
        CategoryInput {
            label,
            counterparty: cp,
            memo: None,
            kind: Some(kind),
            amount,
            holder_name: Some("M FERRAND JEREMIE"),
        }
    }

    fn cat(label: &str, cp: Option<&str>, kind: &str, amount: f64) -> CategorySuggestion {
        suggest(&input(label, cp, kind, amount))
    }

    #[test]
    fn a_known_game_store_is_entertainment_with_confidence() {
        let s = cat(
            "PAYPAL *STEAM GAM",
            Some("PAYPAL *STEAM GAM"),
            "card",
            -6.14,
        );
        assert_eq!(s.category_id, "cat_entertainment_games");
        assert_eq!(s.taxonomy_id, SPENDING_TAXONOMY);
        assert_eq!(s.confidence, HIGH);
        assert!(!s.needs_review());
    }

    #[test]
    fn only_low_confidence_asks_for_review() {
        let kraken = cat("Kraken Kraken", Some("Kraken Kraken"), "transfer", -10.0);
        assert_eq!(kraken.confidence, MEDIUM);
        assert!(!kraken.needs_review());
        assert!(cat("TRAMVENE CENTRO", None, "card", -50.0).needs_review());
    }

    #[test]
    fn a_salary_transfer_is_income_from_employment() {
        let s = cat(
            "APSIDE TECHNOLOGIES SAS — VIREMENT-SALAIRE-DECEMBRE-20-02790",
            Some("APSIDE TECHNOLOGIES SAS"),
            "transfer",
            2165.36,
        );
        assert_eq!(s.category_id, "cat_income_salary");
        assert_eq!(s.taxonomy_id, INCOME_TAXONOMY);
        assert_eq!(s.confidence, HIGH);
    }

    #[test]
    fn a_mobile_plan_direct_debit_is_a_phone_bill() {
        let s = cat(
            "ORANGE — Votre abonnement mobile: 07XXXXX138 (",
            Some("ORANGE"),
            "direct_debit",
            -9.99,
        );
        assert_eq!(s.category_id, "cat_bills_phone");
        assert_eq!(s.confidence, HIGH);
    }

    #[test]
    fn a_card_payment_in_dollars_to_an_unknown_merchant_is_flagged() {
        let s = cat(
            "CONTINUUM GRAPHIC — 15,00 USD / 1 euro = 1,184834123",
            Some("CONTINUUM GRAPHIC"),
            "card",
            -12.66,
        );
        assert_eq!(s.confidence, LOW);
        assert!(s.needs_review());
        let reason = s.reason.unwrap();
        assert!(reason.contains("devise"), "{reason}");
        assert!(reason.contains("non reconnu"), "{reason}");
    }

    #[test]
    fn a_transfer_to_oneself_is_not_called_an_expense_with_confidence() {
        let s = cat(
            "FERRAND JEREMIE — Virement de Monsieur",
            Some("FERRAND JEREMIE"),
            "instant_transfer",
            -800.0,
        );
        assert_eq!(s.category_id, "cat_transfers_own");
        assert_eq!(
            s.confidence, MEDIUM,
            "the whole name is there: account to account"
        );
        assert!(s.reason.unwrap().contains("vous-même"));
        let incoming = cat(
            "MR FERRAND JEREMIE",
            Some("MR FERRAND JEREMIE"),
            "transfer",
            3000.0,
        );
        assert_eq!(incoming.taxonomy_id, INCOME_TAXONOMY);
        assert_eq!(incoming.category_id, "cat_income_transfers_own");
        assert_eq!(incoming.confidence, MEDIUM);
        assert!(!incoming.needs_review());
    }

    #[test]
    fn a_name_that_only_looks_like_the_holders_is_still_looked_at() {
        let s = cat("1FERRAN", Some("1FERRAN"), "transfer", 120.0);
        assert_eq!(s.category_id, "cat_income_transfers_own");
        assert_eq!(s.confidence, LOW);
        assert!(s.needs_review());
    }

    #[test]
    fn a_relative_with_the_same_surname_is_not_taken_for_oneself() {
        let s = cat(
            "MLE FERRAND AMANDINE — CADEAU NOA L",
            Some("MLE FERRAND AMANDINE"),
            "transfer",
            63.2,
        );
        assert_eq!(s.category_id, "cat_income_gifts");
        let out = cat(
            "CCP DOUDOUNE CCP DOUDOUNE — Cadeau papa",
            Some("CCP DOUDOUNE CCP DOUDOUNE"),
            "instant_transfer",
            -15.0,
        );
        assert_eq!(out.category_id, "cat_gifts");
    }

    #[test]
    fn a_reimbursement_label_wins_over_a_truncated_own_name() {
        let s = cat(
            "1FERRAN — Remboursement one plus",
            Some("1FERRAN"),
            "transfer",
            380.53,
        );
        assert_eq!(s.category_id, "cat_income_reimbursements");
        assert_eq!(
            s.confidence, MEDIUM,
            "it comes from oneself: not a sure thing"
        );
    }

    #[test]
    fn money_sent_to_a_crypto_exchange_is_a_transfer_between_own_accounts() {
        let s = cat(
            "Kraken Kraken — AA97 N84G HBIA MLLI kraken.com",
            Some("Kraken Kraken"),
            "transfer",
            -2000.0,
        );
        assert_eq!(s.taxonomy_id, SPENDING_TAXONOMY);
        assert_eq!(s.category_id, "cat_transfers_crypto");
        assert_eq!(s.confidence, MEDIUM);
        assert!(s.reason.unwrap().contains("crypto"));
        let back = cat("Binance — retrait", Some("Binance"), "transfer", 500.0);
        assert_eq!(back.taxonomy_id, INCOME_TAXONOMY);
        assert_eq!(back.category_id, "cat_income_transfers_crypto");
        let broker = cat("DEGIRO", Some("DEGIRO"), "transfer", -300.0);
        assert_eq!(broker.category_id, "cat_transfers_broker");
    }

    #[test]
    fn cash_taxes_and_loans_have_their_own_categories() {
        assert_eq!(
            cat("RETRAIT DAB PARIS", None, "card", -60.0).category_id,
            "cat_cash_withdrawal"
        );
        assert_eq!(
            cat("DGFIP PRELEVEMENT", None, "direct_debit", -120.0).category_id,
            "cat_taxes"
        );
        assert_eq!(
            cat("AMENDE ANTAI", None, "card", -35.0).category_id,
            "cat_taxes_fines"
        );
        assert_eq!(
            cat("ECHEANCE PRET", None, "loan_payment", -726.06).category_id,
            "cat_housing_loan"
        );
        assert_eq!(
            cat("CAF DE L'OISE", None, "transfer", 210.0).category_id,
            "cat_income_benefits"
        );
    }

    #[test]
    fn the_transfer_that_opens_an_account_is_account_to_account() {
        let funding = cat(
            "VIREMENT CREATION COMPTE — BOURSORAMA",
            Some("BOURSORAMA"),
            "transfer",
            2200.0,
        );
        assert_eq!(funding.category_id, "cat_income_transfers_own");
        assert!(!funding.needs_review());
        let check = cat(
            "VIREMENT CREATION COMPTE — M FERRAND J",
            Some("M FERRAND J"),
            "transfer",
            -10.0,
        );
        assert_eq!(check.category_id, "cat_transfers_own");
    }

    #[test]
    fn glasses_are_health_vision() {
        let s = cat("OPTIQUE FOCH 77", Some("OPTIQUE FOCH 77"), "card", -173.0);
        assert_eq!(s.category_id, "cat_health_vision");
        assert_eq!(s.confidence, HIGH);
    }

    #[test]
    fn a_round_card_amount_at_an_unknown_place_asks_whether_it_was_cash() {
        let s = cat("TRAMVENE CENTRO", Some("TRAMVENE CENTRO"), "card", -200.0);
        assert_eq!(s.confidence, LOW);
        assert!(s.reason.unwrap().contains("espèces"));
    }

    #[test]
    fn bank_welcome_bonuses_have_their_own_category() {
        let s = cat("Prime BRS80", None, "other", 80.0);
        assert_eq!(s.category_id, "cat_income_prime");
        let s2 = cat("Offre Mobilite Bancaire", None, "other", 50.0);
        assert_eq!(s2.category_id, "cat_income_prime");
    }

    #[test]
    fn fees_and_interest_follow_the_reader() {
        assert_eq!(
            cat("COTISATION", None, "fee", -2.5).category_id,
            "cat_fees_bank"
        );
        assert_eq!(
            cat("INTERETS", None, "interest", 0.35).category_id,
            "cat_income_interest"
        );
    }

    #[test]
    fn every_suggested_category_exists_in_the_seeded_taxonomies() {
        // the ids the migrations seed: `cat_<key>` is found quoted in one of them
        let seeds = [
            include_str!(
                "../../storage-sqlite/migrations/2026-05-25-000001_spending_module/up.sql"
            ),
            include_str!(
                "../../storage-sqlite/migrations/2026-11-10-000009_transfer_categories/up.sql"
            ),
        ]
        .join("\n");
        let mut keys: Vec<&str> = EXPENSE_RULES
            .iter()
            .chain(INCOME_RULES.iter())
            .map(|r| r.category)
            .collect();
        keys.extend([
            "fees_bank",
            "fees_interest",
            "housing_loan",
            "other_expense",
            "income_other",
            "income_interest",
            "shopping",
            "bills_subscriptions",
            "transfers_own",
            "income_transfers_own",
        ]);
        for key in keys {
            assert!(
                seeds.contains(&format!("'cat_{key}'")),
                "category cat_{key} is not seeded by a migration"
            );
        }
    }
}
