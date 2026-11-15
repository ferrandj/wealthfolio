//! Synthetic Hello bank! documents for the tests: made-up holder, address, IBAN and amounts, the geometry of the real
//! "Relevé de compte chèques" (a BNP Paribas group layout, the first name printed before the last name).

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::fixtures::{dec, make_iban, rib_key, ymd};
use super::fortuneo::fr_space;
use super::pdf::Pdf;

pub const H_BANK: &str = "30004";
pub const H_BRANCH: &str = "00999";
pub const H_ACCOUNT: &str = "00012345678";
/// As the bank prints it: first name, then last name.
pub const H_PRINTED_HOLDER: &str = "M PAUL MARTIN";
/// As the application keeps it: last name, then first name.
pub const H_HOLDER: &str = "M MARTIN PAUL";

#[derive(Clone)]
pub struct HEntry {
    pub day: NaiveDate,
    pub value_day: Option<NaiveDate>,
    pub lines: Vec<String>,
    pub amount: Decimal,
}

pub fn hentry(day: NaiveDate, lines: &[&str], amount: &str) -> HEntry {
    HEntry {
        day,
        value_day: None,
        lines: lines.iter().map(|s| s.to_string()).collect(),
        amount: dec(amount),
    }
}

#[derive(Clone)]
pub struct HSpec {
    pub entries: Vec<HEntry>,
    pub opening: Decimal,
    pub opening_on: NaiveDate,
    pub period_end: NaiveDate,
    pub holder: String,
    pub account: String,
    pub wrong_totals: bool,
    pub title: String,
    /// The title printed at the top ("RELEVE DE COMPTE CHEQUES", "RELEVE DE LIVRET A"…).
    pub heading: String,
    /// The newer layout: the name alone above the RIB's row (no civility, last name first), then a flat's code.
    pub name_above: bool,
}

impl HSpec {
    pub fn new(entries: Vec<HEntry>) -> HSpec {
        HSpec {
            entries,
            opening: dec("500.00"),
            opening_on: ymd(2026, 9, 22),
            period_end: ymd(2026, 10, 15),
            holder: H_PRINTED_HOLDER.into(),
            account: H_ACCOUNT.into(),
            wrong_totals: false,
            title: String::new(),
            heading: "RELEVE DE COMPTE CHEQUES".into(),
            name_above: false,
        }
    }

    pub fn closing(&self) -> Decimal {
        self.opening + self.entries.iter().map(|e| e.amount).sum::<Decimal>()
    }
}

fn dotted(d: NaiveDate) -> String {
    d.format("%d.%m.%Y").to_string()
}

fn long_french(d: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    format!(
        "{} {} {}",
        d.format("%-d"),
        MONTHS[d.format("%m").to_string().parse::<usize>().unwrap() - 1],
        d.format("%Y")
    )
}

pub fn hello_statement_pdf(spec: &HSpec) -> Vec<u8> {
    let mut pdf = Pdf::new(&spec.title);
    let key = rib_key(H_BANK, H_BRANCH, &spec.account);
    let iban = make_iban(&format!("{H_BANK}{H_BRANCH}{}{key}", spec.account), "FR");
    let iban_spaced = iban
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    pdf.text(205.0, 20.0, &spec.heading);
    pdf.text(523.0, 27.0, "P. 1/1");
    pdf.text(
        264.0,
        32.0,
        &format!(
            "du {} au {}",
            long_french(spec.opening_on.succ_opt().unwrap()),
            long_french(spec.period_end)
        ),
    );
    pdf.text(354.0, 80.0, "03101 03166");
    pdf.text(17.0, 87.0, "Hello bank");
    pdf.text(17.0, 100.0, "Tél (1) : 01 00 00 00 00 (numéro non surtaxé)");
    pdf.text(
        17.0,
        151.0,
        &format!("RIB : {H_BANK} {H_BRANCH} {} {key}", spec.account),
    );
    if spec.name_above {
        pdf.text(354.0, 141.0, &spec.holder);
        pdf.text(354.0, 152.0, "A101");
    } else {
        pdf.text(354.0, 152.0, &spec.holder);
    }
    pdf.text(17.0, 163.0, &format!("IBAN : {iban_spaced}"));
    pdf.text(354.0, 163.0, "8 RUE DES TILLEULS");
    pdf.text(17.0, 173.0, "BIC : BNPAFRPPXXX");
    pdf.text(354.0, 175.0, "69003 LYON");
    pdf.text(79.0, 240.0, "Vous trouverez les modifications des conditions tarifaires des produits et services liés à votre compte de dépôt.");
    pdf.text(79.0, 354.0, "Les sommes déposées sur ce compte sont éligibles à la garantie des dépôts du Fonds de Garantie des Dépôts.");
    pdf.text(450.0, 387.0, "Monnaie du compte : EURO");
    pdf.text(31.0, 406.0, "Date");
    pdf.text(316.0, 406.0, "Valeur");
    pdf.text(74.0, 407.0, "Nature des opérations");
    pdf.right(451.0, 407.0, "Débit");
    pdf.right(546.0, 407.0, "Crédit");

    let side = |amount: Decimal| if amount < Decimal::ZERO { 451.0 } else { 546.0 };
    let mut y = 426.0;
    let opening_word = if spec.opening >= Decimal::ZERO {
        "CREDITEUR"
    } else {
        "DEBITEUR"
    };
    pdf.text(
        74.0,
        y,
        &format!("SOLDE {opening_word} AU {}", dotted(spec.opening_on)),
    );
    pdf.right(side(spec.opening), y, &fr_space(spec.opening.abs()));
    y += 15.0;
    for e in &spec.entries {
        pdf.text(31.0, y, &e.day.format("%d.%m").to_string());
        pdf.text(74.0, y, &e.lines[0]);
        pdf.text(
            320.0,
            y,
            &e.value_day.unwrap_or(e.day).format("%d.%m").to_string(),
        );
        pdf.right(side(e.amount), y, &fr_space(e.amount.abs()));
        for extra in &e.lines[1..] {
            y += 11.0;
            pdf.text(74.0, y, extra);
        }
        y += 15.0;
    }
    let mut debits: Decimal = spec
        .entries
        .iter()
        .filter(|e| e.amount < Decimal::ZERO)
        .map(|e| -e.amount)
        .sum();
    let credits: Decimal = spec
        .entries
        .iter()
        .filter(|e| e.amount > Decimal::ZERO)
        .map(|e| e.amount)
        .sum();
    if spec.wrong_totals {
        debits += dec("1.00");
    }
    y += 5.0;
    pdf.text(74.0, y, "TOTAL DES OPERATIONS");
    pdf.right(452.0, y, &fr_space(debits));
    pdf.right(546.0, y, &fr_space(credits));
    y += 17.0;
    let closing = spec.closing();
    let closing_word = if closing >= Decimal::ZERO {
        "CREDITEUR"
    } else {
        "DEBITEUR"
    };
    pdf.text(
        74.0,
        y,
        &format!("SOLDE {closing_word} AU {}", dotted(spec.period_end)),
    );
    pdf.right(side(closing), y, &fr_space(closing.abs()));
    y += 22.0;
    pdf.text(
        40.0,
        y,
        &format!(
            "Montant de votre autorisation de découvert en compte au {} : 0,00 Euro.",
            dotted(spec.period_end)
        ),
    );
    pdf.text(11.0, 794.0, "Hello bank! est une marque de BNP PARIBAS SA au capital de 2 000 000 000 € - Siège social fictif.");
    pdf.bytes()
}

/// A month of everything the statement can show: a credit, a card payment, a direct debit, a transfer to someone,
/// a transfer between the holder's own accounts, a cheque, a fee, and a long label on two lines.
pub fn hello_entries() -> Vec<HEntry> {
    let e = hentry;
    vec![
        e(
            ymd(2026, 9, 25),
            &[
                "VIR SEPA RECU /FRM SOCIETE EXEMPLE SAS",
                "/MOTIF SALAIRE SEPTEMBRE /REF 20260925A",
            ],
            "1800.00",
        ),
        e(
            ymd(2026, 9, 28),
            &["CARTE X1234 27/09 BOULANGERIE TEST LYON"],
            "-3.50",
        ),
        e(
            ymd(2026, 10, 1),
            &[
                "PRLV SEPA OPERATEUR MOBILE EXEMPLE",
                "/MDT 000111 /MOTIF FACTURE OCTOBRE",
                "FR12ZZZ123456",
            ],
            "-19.99",
        ),
        e(
            ymd(2026, 10, 2),
            &[
                "VIR SEPA EMIS /TO LEA DURAND",
                "/MOTIF REMBOURSEMENT RESTAURANT",
            ],
            "-25.00",
        ),
        e(
            ymd(2026, 10, 5),
            &[
                "VIR SEPA RECU /FRM M MARTIN PAUL",
                "/EID /RNF VIREMENT DE PAUL MARTIN",
            ],
            "200.00",
        ),
        e(ymd(2026, 10, 8), &["CHEQUE 0001234"], "-60.00"),
        e(
            ymd(2026, 10, 12),
            &["COTISATION CARTE VISA CLASSIC"],
            "-2.50",
        ),
        e(
            ymd(2026, 10, 14),
            &["RETRAIT DAB 13/10 AGENCE EXEMPLE"],
            "-40.00",
        ),
    ]
}

const FOOTER: &str = "Hello bank! est une marque de BNP PARIBAS SA au capital de 2 000 000 000 € - Siège social fictif - www.hellobank.fr";

/// The RIB of a made-up account of the bank, in four groups ("30004 00999 00012345678 KK").
pub fn h_rib(account: &str) -> String {
    format!(
        "{H_BANK} {H_BRANCH} {account} {}",
        rib_key(H_BANK, H_BRANCH, account)
    )
}

/// The same RIB in one block, as the statements print the other account of a movement.
pub fn h_rib_block(account: &str) -> String {
    h_rib(account).replace(' ', "")
}

pub fn h_iban(account: &str) -> String {
    make_iban(&h_rib_block(account), "FR")
}

fn lines(pdf: &mut Pdf, x: f64, mut y: f64, rows: &[&str]) {
    for row in rows {
        pdf.text(x, y, row);
        y += 12.0;
    }
}

/// "Mon offre": the agreement of the current account (made-up holder and account).
pub fn hello_account_agreement_pdf() -> Vec<u8> {
    let iban = h_iban(H_ACCOUNT);
    let spaced = iban
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let mut pdf = Pdf::new("");
    lines(
        &mut pdf,
        60.0,
        40.0,
        &[
            "Documents composant votre dossier",
            "Convention de signature",
            "Conditions particulières - Convention de compte de dépôt",
        ],
    );
    pdf.text(11.0, 794.0, FOOTER);
    pdf.add_page();
    lines(
        &mut pdf,
        60.0,
        40.0,
        &[
            "DEMANDE DE SOUSCRIPTION D'UNE CONVENTION DE COMPTE DE DEPOT",
            "CONDITIONS PARTICULIERES",
            "TITULAIRE",
            "Civilité : Madame",
            "Nom : DURAND",
            "Prénom : LEA",
            "Nom marital ou d'usage :",
            "Conditions de fonctionnement de votre compte individuel",
            &format!("Numéro de votre compte individuel {spaced}"),
            "- Périodicité de votre relevé de compte électronique : mensuelle",
            "Un versement initial n'est pas exigé. Les livrets font l'objet d'une note d'information.",
        ],
    );
    pdf.text(11.0, 794.0, FOOTER);
    pdf.add_page();
    lines(
        &mut pdf,
        60.0,
        40.0,
        &[
            "CONDITIONS PARTICULIERES - SOUSCRIPTION",
            "Informations commerciales au 04.01.2026",
            "CONDITIONS PARTICULIERES DE VOTRE CONTRAT CARTE DANS LE CADRE DE VOTRE OFFRE HELLO ONE",
        ],
    );
    pdf.text(11.0, 794.0, FOOTER);
    pdf.bytes()
}

/// The contract of a savings account: `livret_a` gives the Livret A (a reservation number, a rate), otherwise the
/// Livret Hello+ (its account number, a rate by slices).
pub fn hello_savings_contract_pdf(livret_a: bool) -> Vec<u8> {
    let product = if livret_a {
        "LIVRET A"
    } else {
        "LIVRET HELLO PLUS"
    };
    let mut pdf = Pdf::new("");
    pdf.text(300.0, 30.0, product);
    pdf.text(300.0, 42.0, "NOTE D'INFORMATION");
    pdf.text(
        300.0,
        54.0,
        "Informations commerciales valables au 09.01.2026",
    );
    lines(
        &mut pdf,
        60.0,
        90.0,
        &[
            "I. L'ESSENTIEL",
            "Les autres livrets (Livret de Développement Durable et Solidaire) font l'objet d'autres conditions.",
        ],
    );
    pdf.text(11.0, 794.0, FOOTER);
    pdf.add_page();
    pdf.text(300.0, 30.0, product);
    pdf.text(300.0, 42.0, "CONDITIONS PARTICULIERES");
    pdf.text(354.0, 80.0, "MME LEA DURAND");
    pdf.text(354.0, 92.0, "8 RUE DES TILLEULS");
    pdf.text(354.0, 104.0, "69003 LYON");
    let funding = h_rib(H_ACCOUNT);
    if livret_a {
        lines(
            &mut pdf,
            60.0,
            140.0,
            &[
                "Vous avez demandé l'ouverture d'un LIVRET A N°30280000000000001 (ci-après dénommé \"LIVRET A\") le 17.02.2026 et nous vous",
                "remercions de votre confiance.",
                "Numéro de réservation de votre LIVRET A : 30280000000000001",
                "Taux annuel du compte au 17.02.2026 : 3,00 %",
                &format!("Versement initial : 10,00 euros par débit du compte n° : {funding}"),
            ],
        );
    } else {
        lines(
            &mut pdf,
            60.0,
            140.0,
            &[
                "Vous avez demandé l'ouverture d'un Compte sur Livret LIVRET HELLO PLUS le 09.01.2026 et nous",
                "vous remercions de votre confiance.",
                &format!(
                    "Numéro de compte de votre Compte sur Livret (compte individuel) : {}.",
                    h_rib("00087654321")
                ),
                &format!("Versement initial : 20,00 euros par débit du compte nº : {funding}."),
                "De 0 à 10 000 euros 0,02%",
                "De 10 000 à 50 000 euros 0,05%",
            ],
        );
    }
    pdf.text(11.0, 794.0, FOOTER);
    pdf.bytes()
}

/// The letter confirming a Livret A is open.
pub fn hello_opening_letter_pdf() -> Vec<u8> {
    let rib = h_rib("00055555555");
    let mut pdf = Pdf::new("");
    pdf.text(330.0, 30.0, "LIVRET A");
    pdf.text(330.0, 42.0, "LETTRE DE CONFIRMATION D'OUVERTURE");
    pdf.text(60.0, 80.0, "AGENCE HELLO BANK - 00999");
    pdf.text(354.0, 120.0, "MME LEA DURAND");
    pdf.text(354.0, 132.0, "8 RUE DES TILLEULS");
    pdf.text(354.0, 144.0, "69003 LYON");
    pdf.text(354.0, 170.0, "le 20 février 2026");
    lines(
        &mut pdf,
        60.0,
        200.0,
        &[
            &format!("Livret A n° : {rib}"),
            "Madame,",
            "Le 17.02.2026, vous nous avez accordé votre confiance en sollicitant l'ouverture d'un Livret A sous le",
            "numéro de réservation 30280000000000001 à votre nom dans notre établissement.",
            "Ayant reçu cette confirmation de l'Administration Fiscale, nous avons le plaisir de vous informer de",
            &format!("l'ouverture du Livret A n° {rib} le 20.02.2026 auprès de BNP Paribas."),
        ],
    );
    pdf.text(
        11.0,
        794.0,
        "BNP PARIBAS SA au capital de 2 000 000 000 € - Siège social fictif",
    );
    pdf.bytes()
}

/// The price list handed with a contract.
pub fn hello_price_list_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new("");
    lines(
        &mut pdf,
        60.0,
        60.0,
        &[
            "CONDITIONS",
            "ET TARIFS",
            "Produits et services",
            "pour les particuliers",
            "JANVIER 2026",
            "Un conseiller Hello bank! se tient à votre entière disposition. La brochure est disponible sur hellobank.fr.",
            "le document d'information tarifaire prévu par l'arrêté du 5 septembre 2018.",
        ],
    );
    pdf.bytes()
}

/// A pre-contractual information note: what it is about under its title, then its date.
pub fn hello_precontract_note_pdf(subject: &str, date_line: &str) -> Vec<u8> {
    let mut pdf = Pdf::new("");
    lines(
        &mut pdf,
        60.0,
        60.0,
        &[
            "NOTE D’INFORMATION PRECONTRACTUELLE",
            subject,
            date_line,
            "1 - L’ESSENTIEL",
            "Le compte de dépôt Hello bank! est ouvert sur les livres de la banque au nom d'un ou de deux titulaires.",
            "2 - CONDITIONS DE SOUSCRIPTION",
            "Droit de rétractation",
            "Vous disposez d'un délai de rétractation de 14 jours calendaires.",
        ],
    );
    pdf.bytes()
}

/// The fee information document: the provider is the parent bank, the offers are Hello bank's.
pub fn hello_fee_information_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new("");
    lines(
        &mut pdf,
        60.0,
        60.0,
        &[
            "Document d’information tarifaire",
            "Nom du prestataire de compte : BANQUE MERE",
            "Intitulé du compte : Compte de dépôt à vue en euros",
            "Date : 1er août 2026",
            "Tenue de compte Gratuit",
            "Offre [Hello One] incluant :",
            "Fourniture d’une carte de débit [Hello Prime avec assurance] Annuellement : 100€",
        ],
    );
    pdf.bytes()
}

/// The personal data notice.
pub fn hello_privacy_notice_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new("");
    lines(
        &mut pdf,
        60.0,
        60.0,
        &[
            "PROTECTION DES DONNEES PERSONNELLES",
            "NOTICE D’INFORMATION",
            "Edition mai 2026",
            "Avant-propos : principaux changements",
            "La banque, en tant que responsable du traitement, notamment à travers ses marques comme Hello Bank!,",
            "est responsable de la collecte et du traitement de vos données personnelles.",
        ],
    );
    pdf.bytes()
}
