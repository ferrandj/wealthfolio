//! A loan rebuilt from the papers the bank sent: the contract (numbered amortization table), an early repayment
//! letter, the dated table printed afterwards. The papers can arrive in any order; they are replayed together and
//! every junction is checked (capital owed before the repayment, interest, end date, total cost...).
//!
//! Pure computation: no I/O, no database. Port of myMoney's `services/loans.rs`.

use std::str::FromStr;

use chrono::{Months, NaiveDate};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn cents(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

pub fn add_months(day: NaiveDate, months: i64) -> NaiveDate {
    let m = Months::new(months.unsigned_abs() as u32);
    if months >= 0 {
        day.checked_add_months(m).unwrap_or(day)
    } else {
        day.checked_sub_months(m).unwrap_or(day)
    }
}

fn fr(day: Option<NaiveDate>) -> String {
    day.map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|| "date inconnue".into())
}

fn euros(v: &Decimal) -> String {
    let s = format!("{:.2}", v.round_dp(2));
    let (int, dec) = s.split_once('.').unwrap_or((&s, "00"));
    let neg = int.starts_with('-');
    let digits: Vec<char> = int.trim_start_matches('-').chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(*c);
    }
    format!("{}{out},{dec}\u{a0}€", if neg { "-" } else { "" })
}

fn pct(v: &Decimal) -> String {
    format!("{} %", v.normalize().to_string().replace('.', ","))
}

fn dec_of(v: &Value, key: &str) -> Option<Decimal> {
    match v.get(key)? {
        Value::String(s) => Decimal::from_str(s.trim()).ok(),
        Value::Number(n) => Decimal::from_str(&n.to_string()).ok(),
        _ => None,
    }
}

fn date_of(v: &Value, key: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(v.get(key)?.as_str()?, "%Y-%m-%d").ok()
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfferRow {
    pub number: i64,
    pub capital: Decimal,
    pub interest: Decimal,
    pub insurance: Decimal,
    pub total: Decimal,
    pub remaining: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatedRow {
    pub date: NaiveDate,
    pub capital: Decimal,
    pub interest: Decimal,
    pub insurance: Decimal,
    pub total: Decimal,
    pub remaining: Decimal,
}

/// The contract: its particular conditions and the numbered table.
#[derive(Debug, Clone, Default)]
pub struct OfferData {
    pub reference: Option<String>,
    pub project: Option<String>,
    pub product: Option<String>,
    pub object: Option<String>,
    pub destination: Option<String>,
    pub borrower: Option<String>,
    pub offer_date: Option<NaiveDate>,
    pub principal: Option<Decimal>,
    pub duration: Option<i64>,
    pub rate: Option<Decimal>,
    pub taeg: Option<Decimal>,
    pub total_cost: Option<Decimal>,
    pub interest_total: Option<Decimal>,
    pub insurance_mandatory: Option<Decimal>,
    pub guarantee_fees: Option<Decimal>,
    pub file_fees: Option<Decimal>,
    pub broker_fees: Option<Decimal>,
    pub insurance_optional: Option<Decimal>,
    pub rows: Vec<OfferRow>,
}

#[derive(Debug, Clone, Default)]
pub struct ScheduleData {
    pub reference: Option<String>,
    pub product: Option<String>,
    pub borrower: Option<String>,
    pub rate: Option<Decimal>,
    pub rows: Vec<DatedRow>,
}

/// An early repayment letter.
#[derive(Debug, Clone)]
pub struct EventData {
    pub borrower: Option<String>,
    pub issued_on: Option<NaiveDate>,
    pub on_date: NaiveDate,
    pub capital_before: Decimal,
    pub last_due_date: Option<NaiveDate>,
    pub payment_before: Option<Decimal>,
    pub end_before: Option<NaiveDate>,
    pub capital_after: Decimal,
    pub first_due_date: Option<NaiveDate>,
    pub first_payment: Option<Decimal>,
    pub end_after: Option<NaiveDate>,
}

impl OfferData {
    /// From the `extra.loan` object of the reader.
    pub fn from_json(v: &Value) -> Self {
        let rows = v
            .get("rows")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        Some(OfferRow {
                            number: r.get("n")?.as_i64()?,
                            capital: dec_of(r, "capital")?,
                            interest: dec_of(r, "interest")?,
                            insurance: dec_of(r, "insurance").unwrap_or_default(),
                            total: dec_of(r, "total")?,
                            remaining: dec_of(r, "remaining")?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            reference: str_of(v, "reference"),
            project: str_of(v, "project"),
            product: str_of(v, "product"),
            object: str_of(v, "object"),
            destination: str_of(v, "destination"),
            borrower: str_of(v, "borrower"),
            offer_date: date_of(v, "offer_date"),
            principal: dec_of(v, "principal"),
            duration: v.get("duration").and_then(Value::as_i64),
            rate: dec_of(v, "rate"),
            taeg: dec_of(v, "taeg"),
            total_cost: dec_of(v, "total_cost"),
            interest_total: dec_of(v, "interest_total"),
            insurance_mandatory: dec_of(v, "insurance_mandatory"),
            guarantee_fees: dec_of(v, "guarantee_fees"),
            file_fees: dec_of(v, "file_fees"),
            broker_fees: dec_of(v, "broker_fees"),
            insurance_optional: dec_of(v, "insurance_optional"),
            rows,
        }
    }
}

impl ScheduleData {
    pub fn from_json(v: &Value) -> Self {
        let rows = v
            .get("rows")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        Some(DatedRow {
                            date: date_of(r, "date")?,
                            capital: dec_of(r, "capital")?,
                            interest: dec_of(r, "interest")?,
                            insurance: dec_of(r, "insurance").unwrap_or_default(),
                            total: dec_of(r, "total")?,
                            remaining: dec_of(r, "remaining")?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            reference: str_of(v, "reference"),
            product: str_of(v, "product"),
            borrower: str_of(v, "borrower"),
            rate: dec_of(v, "rate"),
            rows,
        }
    }
}

impl EventData {
    pub fn from_json(v: &Value) -> Option<Self> {
        let e = v.get("event")?;
        let before = e.get("before")?;
        let after = e.get("after")?;
        Some(Self {
            borrower: str_of(v, "borrower"),
            issued_on: date_of(e, "issued_on"),
            on_date: date_of(e, "on_date")?,
            capital_before: dec_of(before, "capital")?,
            last_due_date: date_of(before, "due"),
            payment_before: dec_of(before, "payment"),
            end_before: date_of(before, "end"),
            capital_after: dec_of(after, "capital")?,
            first_due_date: date_of(after, "due"),
            first_payment: dec_of(after, "payment"),
            end_after: date_of(after, "end"),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanRow {
    /// installment | prepayment
    pub kind: String,
    pub date: Option<NaiveDate>,
    pub number: Option<i64>,
    pub capital: Decimal,
    pub interest: Decimal,
    pub insurance: Decimal,
    pub total: Decimal,
    pub remaining: Decimal,
    /// contrat | calculé | échéancier | arrêté
    pub source: String,
    /// paid | next | future
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanCheck {
    /// ok | warn | info
    pub status: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub interest: Decimal,
    pub insurance: Decimal,
    pub months: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub as_of: NaiveDate,
    pub owed: Option<Decimal>,
    pub end_date: Option<NaiveDate>,
    pub paid_count: usize,
    pub remaining_count: usize,
    pub total_count: usize,
    pub next: Option<PlanRow>,
    pub dated: bool,
    pub future_interest: Decimal,
    pub future_insurance: Decimal,
    pub future_total: Decimal,
    pub paid_total: Decimal,
    pub paid_interest: Decimal,
    pub lifetime_interest: Decimal,
    pub repaid_early: Decimal,
    pub saved: Option<Saved>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub rows: Vec<PlanRow>,
    pub checks: Vec<PlanCheck>,
    pub summary: Option<PlanSummary>,
}

impl Plan {
    pub fn problems(&self) -> usize {
        self.checks.iter().filter(|c| c.status == "warn").count()
    }
}

/// Everything known about one loan.
#[derive(Debug, Clone, Default)]
pub struct LoanInput {
    pub offer: Option<OfferData>,
    pub schedule: Vec<DatedRow>,
    pub schedule_rate: Option<Decimal>,
    pub events: Vec<EventData>,
    /// The date of instalment n°1, typed by the person (it wins over what the papers allow to infer).
    pub first_due: Option<NaiveDate>,
}

impl LoanInput {
    pub fn rate(&self) -> Option<Decimal> {
        self.offer
            .as_ref()
            .and_then(|o| o.rate)
            .or(self.schedule_rate)
            .filter(|r| !r.is_zero())
    }

    /// The date of instalment n°1: typed, or deduced from a dated paper that points at a numbered instalment.
    pub fn infer_first_due(&self) -> Option<NaiveDate> {
        if let Some(d) = self.first_due {
            return Some(d);
        }
        let offer = &self.offer.as_ref()?.rows;
        if offer.is_empty() {
            return None;
        }
        for ev in &self.events {
            if let Some(last) = ev.last_due_date {
                if let Some(r) = offer.iter().find(|r| r.remaining == ev.capital_before) {
                    return Some(add_months(last, -(r.number - 1)));
                }
            }
        }
        if let (Some(first), true) = (self.schedule.first(), self.events.is_empty()) {
            let balance = first.remaining + first.capital;
            if let Some(r) = offer.iter().find(|r| r.remaining == balance) {
                return Some(add_months(first.date, -r.number));
            }
        }
        None
    }
}

fn row(
    kind: &str,
    date: Option<NaiveDate>,
    number: Option<i64>,
    parts: (Decimal, Decimal, Decimal, Decimal),
    remaining: Decimal,
    source: &str,
) -> PlanRow {
    PlanRow {
        kind: kind.into(),
        date,
        number,
        capital: parts.0,
        interest: parts.1,
        insurance: parts.2,
        total: parts.3,
        remaining,
        source: source.into(),
        status: "future".into(),
        note: String::new(),
    }
}

fn check(status: &str, text: impl Into<String>) -> PlanCheck {
    PlanCheck {
        status: status.into(),
        text: text.into(),
    }
}

/// Instalments of a fixed-rate loan from `capital`: interest = capital × rate / 12 (rounded), the rest of the
/// payment repays capital, the last instalment clears what is left.
pub fn project(
    mut capital: Decimal,
    rate: Decimal,
    payment: Decimal,
    insurance: Decimal,
    first_due: NaiveDate,
    source: &str,
) -> Vec<PlanRow> {
    let mut rows = vec![];
    for k in 0..1200 {
        if capital <= Decimal::ZERO {
            break;
        }
        let interest = cents(capital * rate / Decimal::from(1200));
        let mut principal = payment - insurance - interest;
        if principal <= Decimal::ZERO {
            break;
        }
        if principal >= capital || k == 1199 {
            principal = capital;
        }
        capital -= principal;
        rows.push(row(
            "installment",
            Some(add_months(first_due, k)),
            None,
            (
                principal,
                interest,
                insurance,
                principal + interest + insurance,
            ),
            capital,
            source,
        ));
    }
    rows
}

fn check_interest(
    rows: &[(Decimal, Decimal, Decimal)], // interest, remaining, capital
    rate: Decimal,
    label: &str,
    checks: &mut Vec<PlanCheck>,
    start: Decimal,
) {
    let mut balance = start;
    let mut off = 0;
    for (i, (interest, remaining, _)) in rows.iter().enumerate() {
        let expected = cents(balance * rate / Decimal::from(1200));
        if (expected - *interest).abs() > Decimal::new(1, 2) && i + 1 < rows.len() {
            off += 1;
        }
        balance = *remaining;
    }
    let n = rows.len().saturating_sub(1);
    if off == 0 {
        checks.push(check(
            "ok",
            format!(
                "Taux de {} : les intérêts de {n} échéances {label} se recalculent à un centime près.",
                pct(&rate)
            ),
        ));
    } else {
        checks.push(check(
            "warn",
            format!(
                "Taux de {} : {off} échéance(s) {label} ont des intérêts différents du calcul capital × taux / 12.",
                pct(&rate)
            ),
        ));
    }
}

fn check_offer(offer: &OfferData, rate: Option<Decimal>, checks: &mut Vec<PlanCheck>) {
    let capital: Decimal = offer.rows.iter().map(|r| r.capital).sum();
    let interest: Decimal = offer.rows.iter().map(|r| r.interest).sum();
    let mut problems: Vec<String> = vec![];
    if let Some(p) = offer.principal {
        if capital != p {
            problems.push("la somme des amortissements n'est pas le montant emprunté".into());
        }
    }
    if let Some(m) = offer.duration.filter(|m| *m != 0) {
        if offer.rows.len() as i64 != m {
            problems.push(format!("{} échéances pour {m} mois", offer.rows.len()));
        }
    }
    if let Some(it) = offer.interest_total {
        if interest != it {
            problems.push(
                "les intérêts du tableau ne sont pas ceux des conditions particulières".into(),
            );
        }
    }
    for r in &offer.rows {
        if r.capital + r.interest + r.insurance != r.total {
            problems.push(format!("l'échéance n° {} ne s'additionne pas", r.number));
            break;
        }
    }
    if problems.is_empty() {
        checks.push(check(
            "ok",
            format!(
                "Contrat : {} échéances, {} de capital remboursé (= montant emprunté), {} d'intérêts (= conditions particulières).",
                offer.rows.len(),
                euros(&capital),
                euros(&interest)
            ),
        ));
    } else {
        checks.push(check(
            "warn",
            format!("Contrat : {}.", problems.join(" ; ")),
        ));
    }
    if let (Some(tc), Some(it), Some(im), Some(gf)) = (
        offer.total_cost,
        offer.interest_total,
        offer.insurance_mandatory,
        offer.guarantee_fees,
    ) {
        let total = it
            + im
            + gf
            + offer.file_fees.unwrap_or_default()
            + offer.broker_fees.unwrap_or_default();
        checks.push(if total == tc {
            check(
                "ok",
                format!(
                    "Coût total du crédit {} = intérêts {} + assurance obligatoire {} + frais de garantie {}.",
                    euros(&tc),
                    euros(&it),
                    euros(&im),
                    euros(&gf)
                ),
            )
        } else {
            check(
                "warn",
                format!(
                    "Le coût total annoncé ({}) n'est pas la somme de ses composantes ({}).",
                    euros(&tc),
                    euros(&total)
                ),
            )
        });
    }
    if let Some(r) = rate {
        let rows: Vec<_> = offer
            .rows
            .iter()
            .map(|r| (r.interest, r.remaining, r.capital))
            .collect();
        if let Some(first) = rows.first() {
            check_interest(
                &rows,
                r,
                "du contrat",
                checks,
                offer.principal.unwrap_or(first.1 + first.2),
            );
        }
    }
}

/// Replay the papers and return the plan with its checks and summary.
pub fn build_plan(input: &LoanInput, today: NaiveDate) -> Plan {
    let offer_rows: Vec<OfferRow> = input
        .offer
        .as_ref()
        .map(|o| {
            let mut r = o.rows.clone();
            r.sort_by_key(|x| x.number);
            r
        })
        .unwrap_or_default();
    let mut schedule = input.schedule.clone();
    schedule.sort_by_key(|r| r.date);
    let mut evs = input.events.clone();
    evs.sort_by_key(|e| e.on_date);
    let first_due = input.infer_first_due();
    let rate = input.rate();
    let insurance = offer_rows
        .last()
        .map(|r| r.insurance)
        .or(schedule.last().map(|r| r.insurance))
        .unwrap_or_default();

    let mut checks: Vec<PlanCheck> = vec![];
    let mut rows: Vec<PlanRow> = vec![];

    // 1. the contract
    for i in &offer_rows {
        let when = first_due.map(|f| add_months(f, i.number - 1));
        rows.push(row(
            "installment",
            when,
            Some(i.number),
            (i.capital, i.interest, i.insurance, i.total),
            i.remaining,
            "contrat",
        ));
    }
    if let Some(offer) = &input.offer {
        if !offer_rows.is_empty() {
            check_offer(offer, rate, &mut checks);
            if first_due.is_none() {
                checks.push(check("info", "Les dates des échéances du contrat ne sont pas connues : dépose l'arrêté de remboursement anticipé ou le tableau daté, ou indique la date de la première échéance."));
            }
        }
    }

    // 2. early repayments: the contract stops where the capital owed matches, the new plan starts from what is left
    let mut start_of_new: Option<usize> = None;
    for ev in &evs {
        let cut = (0..rows.len())
            .rev()
            .find(|&k| rows[k].kind == "installment" && rows[k].remaining == ev.capital_before);
        if let Some(cut) = cut {
            let label = match rows[cut].number.filter(|n| *n != 0) {
                Some(n) => format!("échéance n° {n}"),
                None => "échéance".into(),
            };
            let at = rows[cut]
                .date
                .map(|d| format!(" ({})", fr(Some(d))))
                .unwrap_or_default();
            checks.push(check(
                "ok",
                format!(
                    "Remboursement anticipé du {} : le capital restant dû avant ({}) est exactement celui du plan après l'{label}{at}.",
                    fr(Some(ev.on_date)),
                    euros(&ev.capital_before)
                ),
            ));
            if let (Some(rd), Some(ld)) = (rows[cut].date, ev.last_due_date) {
                if rd != ld {
                    checks.push(check(
                        "warn",
                        format!(
                            "La dernière échéance avant le remboursement est le {} dans le courrier, le {} dans le plan.",
                            fr(Some(ld)),
                            fr(Some(rd))
                        ),
                    ));
                }
            }
            rows.truncate(cut + 1);
        } else if !rows.is_empty() {
            checks.push(check(
                "warn",
                format!(
                    "Le capital restant dû avant le remboursement anticipé du {} ({}) ne correspond à aucune échéance du plan : une échéance ou un autre remboursement manque peut-être.",
                    fr(Some(ev.on_date)),
                    euros(&ev.capital_before)
                ),
            ));
            rows.retain(|r| matches!((r.date, ev.last_due_date), (Some(a), Some(b)) if a <= b));
        } else {
            checks.push(check(
                "info",
                format!(
                    "Remboursement anticipé du {} : sans le contrat, impossible de vérifier le capital restant dû avant ({}).",
                    fr(Some(ev.on_date)),
                    euros(&ev.capital_before)
                ),
            ));
        }
        if let (Some(eb), false, Some(f)) = (ev.end_before, offer_rows.is_empty(), first_due) {
            let planned_end = add_months(f, offer_rows.len() as i64 - 1);
            checks.push(check(
                if planned_end == eb { "ok" } else { "warn" },
                format!(
                    "Date de fin avant remboursement : {} dans le courrier, {} d'après le contrat ({} échéances).",
                    fr(Some(eb)),
                    fr(Some(planned_end)),
                    offer_rows.len()
                ),
            ));
        }
        let repaid = ev.capital_before - ev.capital_after;
        let mut prepay = row(
            "prepayment",
            Some(ev.on_date),
            None,
            (repaid, Decimal::ZERO, Decimal::ZERO, repaid),
            ev.capital_after,
            "arrêté",
        );
        prepay.note = format!(
            "Remboursement anticipé partiel de {} : capital restant dû {} → {}",
            euros(&repaid),
            euros(&ev.capital_before),
            euros(&ev.capital_after)
        );
        rows.push(prepay);
        let payment = ev.first_payment.filter(|p| !p.is_zero());
        if let (Some(r), Some(fd), Some(payment)) = (rate, ev.first_due_date, payment) {
            let new = project(ev.capital_after, r, payment, insurance, fd, "calculé");
            let n_new = new.len();
            let last_date = new.last().and_then(|x| x.date);
            rows.extend(new);
            start_of_new = Some(rows.len() - n_new);
            if let (true, Some(ea)) = (n_new > 0, ev.end_after) {
                let same = last_date == Some(ea);
                checks.push(check(
                    if same { "ok" } else { "warn" },
                    format!(
                        "Après le remboursement : {n_new} échéances de {} au taux de {} mènent au {} ({} {} dans le courrier).",
                        euros(&payment),
                        pct(&r),
                        fr(last_date),
                        if same { "comme" } else { "courrier :" },
                        fr(Some(ea))
                    ),
                ));
            }
        } else if ev.first_due_date.is_some() && payment.is_some() && rate.is_none() {
            checks.push(check("info", "Le taux du prêt est inconnu (dépose l'offre ou le tableau daté) : l'échéancier après le remboursement ne peut pas être calculé."));
        }
    }

    // 3. the dated table takes over from its first date and must start from the capital the plan has reached
    if let Some(first) = schedule.first().cloned() {
        let first_date = first.date;
        let balance = first.remaining + first.capital;
        if let Some(r) = rate {
            let srows: Vec<_> = schedule
                .iter()
                .map(|s| (s.interest, s.remaining, s.capital))
                .collect();
            check_interest(&srows, r, "du tableau daté", &mut checks, balance);
        }
        let before: Vec<PlanRow> = rows
            .iter()
            .filter(|r| r.date.map(|d| d < first_date).unwrap_or(false))
            .cloned()
            .collect();
        if !offer_rows.is_empty()
            && before.is_empty()
            && evs.is_empty()
            && rows.iter().any(|r| r.date.is_none())
        {
            let near_text = offer_rows
                .iter()
                .min_by_key(|o| (o.remaining - balance).abs())
                .map(|near| {
                    format!(
                        " (la plus proche : n° {}, {})",
                        near.number,
                        euros(&near.remaining)
                    )
                })
                .unwrap_or_default();
            checks.push(check(
                "warn",
                format!(
                    "Le tableau daté part de {} au {} mais aucune échéance du contrat ne mène à ce capital{}. Un remboursement anticipé a probablement eu lieu : dépose son courrier pour que tout soit reconstitué.",
                    euros(&balance),
                    fr(Some(first_date)),
                    near_text
                ),
            ));
            rows.clear();
        } else if let Some(last) = before.last() {
            let gap = last.remaining - balance;
            if gap.is_zero() {
                checks.push(check(
                    "ok",
                    format!(
                        "Le tableau daté part de {} au {} : c'est exactement le capital auquel mène le plan ({}) après le {}.",
                        euros(&balance),
                        fr(Some(first_date)),
                        last.source,
                        fr(last.date)
                    ),
                ));
            } else if gap.abs() <= Decimal::new(5, 2) {
                checks.push(check(
                    "ok",
                    format!(
                        "Le tableau daté part de {} ; le plan calculé donne {} (écart d'arrondi de {}).",
                        euros(&balance),
                        euros(&last.remaining),
                        euros(&gap.abs())
                    ),
                ));
            } else {
                let hint = if evs.is_empty() && gap > Decimal::ZERO {
                    " Un remboursement anticipé manque peut-être (dépose son courrier)."
                } else {
                    ""
                };
                checks.push(check(
                    "warn",
                    format!(
                        "Le tableau daté part de {} au {} mais le plan donne {} : écart de {}.{hint}",
                        euros(&balance),
                        fr(Some(first_date)),
                        euros(&last.remaining),
                        euros(&gap.abs())
                    ),
                ));
            }
        } else if rows.is_empty() {
            checks.push(check(
                "info",
                format!(
                    "Le tableau daté part de {} au {} ; sans contrat ni courrier, rien d'antérieur n'est reconstitué.",
                    euros(&balance),
                    fr(Some(first_date))
                ),
            ));
        }
        if let Some(start) = start_of_new {
            let computed: Vec<&PlanRow> = rows
                .iter()
                .skip(start)
                .filter(|r| r.date.map(|d| d >= first_date).unwrap_or(false))
                .collect();
            let max = computed
                .iter()
                .zip(schedule.iter())
                .map(|(c, s)| (c.remaining - s.remaining).abs())
                .max();
            let same_end = computed.len() == schedule.len()
                && computed.last().and_then(|c| c.date) == schedule.last().map(|s| s.date);
            if let (false, true, Some(max)) = (
                computed.is_empty(),
                same_end,
                max.filter(|m| *m <= Decimal::new(10, 2)),
            ) {
                checks.push(check(
                    "ok",
                    format!(
                        "Le plan recalculé retombe sur le tableau daté : mêmes {} échéances, écart maximal de {} sur le capital restant dû.",
                        schedule.len(),
                        euros(&max)
                    ),
                ));
            } else if !computed.is_empty() {
                checks.push(check("warn", "Le plan recalculé après le remboursement ne correspond pas au tableau daté (dates ou capitaux différents)."));
            }
        }
        rows.retain(|r| !r.date.map(|d| d >= first_date).unwrap_or(false));
        for s in &schedule {
            rows.push(row(
                "installment",
                Some(s.date),
                None,
                (s.capital, s.interest, s.insurance, s.total),
                s.remaining,
                "échéancier",
            ));
        }
        let last_due = schedule.last().map(|s| s.date);
        match evs.last().and_then(|e| e.end_after) {
            Some(ea) if last_due != Some(ea) => checks.push(check(
                "warn",
                format!(
                    "Le tableau daté se termine le {}, le courrier annonce le {}.",
                    fr(last_due),
                    fr(Some(ea))
                ),
            )),
            _ if !evs.is_empty() => checks.push(check(
                "ok",
                format!(
                    "Fin du prêt le {} : la date du tableau daté et celle du courrier de remboursement anticipé concordent.",
                    fr(last_due)
                ),
            )),
            _ => {}
        }
    } else if !evs.is_empty() && !rows.is_empty() {
        checks.push(check("info", "Pas de tableau daté : l'échéancier après le remboursement est calculé à partir du courrier et du taux."));
    }

    // statuses: paid / next / future
    let mut seen_next = false;
    for r in rows.iter_mut() {
        let Some(d) = r.date else { continue };
        if d <= today {
            r.status = "paid".into();
        } else if r.kind == "installment" && !seen_next {
            r.status = "next".into();
            seen_next = true;
        }
    }

    let mut plan = Plan {
        rows,
        checks,
        summary: None,
    };
    finish(input, &mut plan, today, &offer_rows, &evs, first_due);
    plan
}

fn finish(
    input: &LoanInput,
    plan: &mut Plan,
    today: NaiveDate,
    offer: &[OfferRow],
    evs: &[EventData],
    first_due: Option<NaiveDate>,
) {
    let inst: Vec<&PlanRow> = plan
        .rows
        .iter()
        .filter(|r| r.kind == "installment")
        .collect();
    if inst.is_empty() {
        return;
    }
    let principal = input.offer.as_ref().and_then(|o| o.principal);
    let rows = &plan.rows;
    let mut owed = if rows[0].kind == "installment" {
        Some(rows[0].remaining + rows[0].capital)
    } else {
        principal
    };
    for r in rows {
        if r.date.map(|d| d <= today).unwrap_or(false) {
            owed = Some(r.remaining);
        }
    }
    let sum =
        |rs: &[&PlanRow], f: fn(&PlanRow) -> Decimal| rs.iter().map(|r| f(r)).sum::<Decimal>();
    let past: Vec<&PlanRow> = inst
        .iter()
        .copied()
        .filter(|r| r.date.map(|d| d <= today).unwrap_or(false))
        .collect();
    let future: Vec<&PlanRow> = inst
        .iter()
        .copied()
        .filter(|r| r.date.map(|d| d > today).unwrap_or(false))
        .collect();
    let end = inst.last().and_then(|r| r.date);
    let mut summary = PlanSummary {
        as_of: today,
        owed,
        end_date: end,
        paid_count: past.len(),
        remaining_count: future.len(),
        total_count: inst.len(),
        next: future.first().map(|r| (*r).clone()),
        dated: rows.iter().all(|r| r.date.is_some()),
        future_interest: sum(&future, |r| r.interest),
        future_insurance: sum(&future, |r| r.insurance),
        future_total: sum(&future, |r| r.total),
        paid_total: sum(&past, |r| r.total),
        paid_interest: sum(&past, |r| r.interest),
        lifetime_interest: sum(&inst, |r| r.interest),
        repaid_early: rows
            .iter()
            .filter(|r| r.kind == "prepayment")
            .map(|r| r.capital)
            .sum(),
        saved: None,
    };
    if let (Some(ev), false, Some(fd)) = (evs.first(), offer.is_empty(), first_due) {
        if let Some(cut) = offer.iter().find(|o| o.remaining == ev.capital_before) {
            let old: Vec<&OfferRow> = offer.iter().filter(|o| o.number > cut.number).collect();
            let limit = add_months(fd, cut.number - 1);
            let after: Vec<&PlanRow> = inst
                .iter()
                .copied()
                .filter(|r| r.date.map(|d| d > limit).unwrap_or(false))
                .collect();
            summary.saved = Some(Saved {
                interest: old.iter().map(|o| o.interest).sum::<Decimal>()
                    - after.iter().map(|r| r.interest).sum::<Decimal>(),
                insurance: old.iter().map(|o| o.insurance).sum::<Decimal>()
                    - after.iter().map(|r| r.insurance).sum::<Decimal>(),
                months: old.len() as i64 - after.len() as i64,
            });
        }
    }
    let all_dated = summary.dated;
    let last_zero = inst.last().map(|r| r.remaining.is_zero()).unwrap_or(false);
    let schedule_used = rows.iter().any(|r| r.source == "échéancier");
    plan.summary = Some(summary);
    if last_zero && all_dated {
        let ok = plan.problems() == 0;
        plan.checks.push(check(
            if ok { "ok" } else { "warn" },
            format!(
                "En appliquant le contrat{}{}, le prêt est entièrement remboursé le {} (capital restant dû 0,00 €).{}",
                if evs.is_empty() { "" } else { ", le remboursement anticipé" },
                if schedule_used { " et le tableau daté" } else { "" },
                fr(end),
                if ok { "" } else { " Des incohérences sont signalées ci-dessus." }
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }
    fn day(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    /// The contract of the real loan the tests are modelled on: 122 432 € at 3.44 % over 243 months, instalment
    /// 726.06 € of which 25.83 € of insurance, the last instalment adjusted so that the total stays the same.
    fn contract() -> OfferData {
        let (rate, insurance, payment) = (d("3.44"), d("25.83"), d("726.06"));
        let mut capital = d("122432.00");
        let mut rows = vec![];
        for n in 1..=243 {
            let interest = cents(capital * rate / Decimal::from(1200));
            let mut principal = payment - insurance - interest;
            if n == 243 {
                principal = capital;
            }
            capital -= principal;
            rows.push(OfferRow {
                number: n,
                capital: principal,
                interest: if n == 243 {
                    payment - insurance - principal
                } else {
                    interest
                },
                insurance,
                total: payment,
                remaining: capital,
            });
        }
        let interest_total = rows.iter().map(|r| r.interest).sum();
        OfferData {
            reference: Some("FT14103062".into()),
            principal: Some(d("122432.00")),
            duration: Some(243),
            rate: Some(rate),
            interest_total: Some(interest_total),
            rows,
            ..Default::default()
        }
    }

    fn letter() -> EventData {
        EventData {
            borrower: None,
            issued_on: Some(day(2026, 6, 8)),
            on_date: day(2026, 6, 6),
            capital_before: d("117086.69"),
            last_due_date: Some(day(2026, 5, 15)),
            payment_before: Some(d("726.06")),
            end_before: Some(day(2045, 5, 15)),
            capital_after: d("104812.15"),
            first_due_date: Some(day(2026, 6, 15)),
            first_payment: Some(d("725.56")),
            end_after: Some(day(2042, 9, 15)),
        }
    }

    #[test]
    fn the_modelled_contract_reproduces_the_real_table() {
        let c = contract();
        // the 15th instalment leaves exactly what the bank's letter says is owed
        assert_eq!(c.rows[14].remaining, d("117086.69"));
        assert_eq!(c.rows[0].interest, d("350.97"));
        assert_eq!(c.rows[242].remaining, Decimal::ZERO);
    }

    #[test]
    fn the_contract_alone_has_no_dates_and_says_so() {
        let input = LoanInput {
            offer: Some(contract()),
            ..Default::default()
        };
        let plan = build_plan(&input, day(2026, 11, 10));
        assert_eq!(plan.rows.len(), 243);
        assert!(plan.rows.iter().all(|r| r.date.is_none()));
        assert_eq!(plan.problems(), 0);
        assert!(plan.checks.iter().any(|c| c.status == "info"));
    }

    #[test]
    fn the_letter_dates_the_contract_and_the_plan_continues_to_the_announced_end() {
        let input = LoanInput {
            offer: Some(contract()),
            events: vec![letter()],
            ..Default::default()
        };
        assert_eq!(input.infer_first_due(), Some(day(2025, 3, 15)));
        let plan = build_plan(&input, day(2026, 11, 10));
        assert_eq!(plan.problems(), 0, "{:#?}", plan.checks);
        let s = plan.summary.clone().unwrap();
        // 15 instalments of the contract + the repayment + 196 computed instalments (the letter says the end is
        // 15/09/2042)
        assert_eq!(s.total_count, 15 + 196);
        assert_eq!(s.end_date, Some(day(2042, 9, 15)));
        assert_eq!(s.repaid_early, d("12274.54"));
        // today (10/11/2026): 5 instalments since the repayment (15/06 .. 15/10), so 20 paid
        assert_eq!(s.paid_count, 20);
        assert_eq!(s.next.as_ref().unwrap().date, Some(day(2026, 11, 15)));
        let saved = s.saved.unwrap();
        assert!(saved.months > 0 && saved.interest > Decimal::ZERO);
    }

    fn view_of(today: NaiveDate) -> crate::statements::LoanView {
        let input = LoanInput {
            offer: Some(contract()),
            events: vec![letter()],
            ..Default::default()
        };
        let plan = build_plan(&input, today);
        crate::statements::LoanView {
            key: "FT14103062".into(),
            label: "Prêt".into(),
            borrower: None,
            product: None,
            principal: input.offer.as_ref().and_then(|o| o.principal),
            duration: input.offer.as_ref().and_then(|o| o.duration),
            rate: input.rate(),
            taeg: None,
            documents: vec![],
            first_due: None,
            inferred_first_due: input.infer_first_due(),
            plan,
            liability_id: None,
            linked_asset_id: None,
        }
    }

    #[test]
    fn the_contract_becomes_the_terms_of_a_liability() {
        let setup = crate::statements::setup_of(&view_of(day(2026, 11, 10))).unwrap();
        assert_eq!(setup.original_amount, Some(122432.0));
        assert_eq!(setup.origination_date, Some(day(2025, 2, 15)));
        assert_eq!(setup.interest_rate, Some(3.44));
        let schedule = setup.schedule.unwrap();
        assert_eq!(schedule.first_payment_date, Some(day(2025, 3, 15)));
        assert_eq!(schedule.amortization_months, Some(243));
        // 726.06 € per instalment, of which 25.83 € of insurance that is not part of the loan
        assert_eq!(schedule.payment_amount, Some(700.23));
        assert_eq!(schedule.escrow_amount, Some(25.83));
    }

    #[test]
    fn the_first_instalment_is_read_in_the_bank_statements() {
        use crate::statements::{first_due_from_debits, BankDebit};
        let view = view_of(day(2026, 11, 10));
        let total = view
            .plan
            .rows
            .iter()
            .find(|r| r.kind == "installment")
            .unwrap()
            .total;
        let debit = |date, amount, label: &str| BankDebit {
            date,
            amount,
            label: label.into(),
        };
        let debits = vec![
            // the right amount, but not a loan
            debit(day(2025, 1, 10), total, "CARTE SUPERMARCHE"),
            // a loan label, but not the instalment
            debit(
                day(2025, 2, 15),
                Decimal::new(1200, 2),
                "PRLV ECHEANCE PRET",
            ),
            debit(day(2025, 4, 15), total, "PRLV ECHEANCE PRET"),
            debit(day(2025, 3, 15), total, "PRLV ECHEANCE PRET"),
        ];
        assert_eq!(
            first_due_from_debits(&view, &debits),
            Some(day(2025, 3, 15))
        );
        assert_eq!(first_due_from_debits(&view, &debits[..2]), None);
    }

    #[test]
    fn the_repayment_and_the_new_instalment_become_dated_changes() {
        use crate::statements::{changes_of, LoanChange};
        let changes = changes_of(&view_of(day(2026, 11, 10)));
        assert_eq!(
            changes,
            vec![
                LoanChange::Repayment {
                    date: day(2026, 6, 6),
                    amount: 12274.54,
                    after: 104812.15,
                    // the plan after the first instalment that follows: the figure the liability is held to
                    next: Some((day(2026, 6, 15), 104412.88)),
                },
                LoanChange::Payment {
                    date: day(2026, 6, 15),
                    amount: 699.73,
                },
            ]
        );
    }

    #[test]
    fn an_instalment_not_yet_paid_is_not_recorded() {
        use crate::statements::{changes_of, LoanChange};
        // on 10/06/2026 the first instalment after the repayment (15/06) has not been paid yet
        let changes = changes_of(&view_of(day(2026, 6, 10)));
        assert!(matches!(
            changes.first(),
            Some(LoanChange::Repayment { next: None, .. })
        ));
    }

    #[test]
    fn a_repayment_that_matches_no_instalment_is_reported() {
        let mut ev = letter();
        ev.capital_before = d("100000.00");
        let input = LoanInput {
            offer: Some(contract()),
            events: vec![ev],
            first_due: Some(day(2025, 3, 15)),
            ..Default::default()
        };
        let plan = build_plan(&input, day(2026, 11, 10));
        assert!(plan.problems() >= 1);
    }

    #[test]
    fn a_wrong_end_date_in_the_letter_is_reported() {
        let mut ev = letter();
        ev.end_after = Some(day(2043, 1, 15));
        let input = LoanInput {
            offer: Some(contract()),
            events: vec![ev],
            ..Default::default()
        };
        assert!(build_plan(&input, day(2026, 11, 10)).problems() >= 1);
    }

    #[test]
    fn a_typed_first_due_date_dates_the_contract() {
        let input = LoanInput {
            offer: Some(contract()),
            first_due: Some(day(2025, 3, 15)),
            ..Default::default()
        };
        let plan = build_plan(&input, day(2026, 11, 10));
        let s = plan.summary.unwrap();
        assert_eq!(s.end_date, Some(day(2045, 5, 15)));
        assert_eq!(s.paid_count, 20);
        assert_eq!(s.owed, Some(plan.rows[19].remaining));
    }
}
