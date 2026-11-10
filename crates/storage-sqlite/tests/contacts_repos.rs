//! Contacts against a real, migrated SQLite database: a contact is matched to imported operations by IBAN or printed
//! name, several accounts hang on one contact, accounts of Wealthfolio can be tied to it, and deletions leave the
//! operations in place.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
mod common;

use diesel::prelude::*;
use std::sync::Arc;

use wealthfolio_core::contacts::{
    ContactInput, ContactsRepositoryTrait, ContactsService, ContactsServiceTrait,
    CounterpartyRecorderTrait, NewContact, NewContactIdentifier, OperationCounterparty,
};
use wealthfolio_storage_sqlite::contacts::ContactsRepository;
use wealthfolio_storage_sqlite::db;

const IBAN_A: &str = "FR76 3000 6000 0112 3456 7890 189";
const IBAN_B: &str = "DE89 3704 0044 0532 0130 00";

struct Env {
    _dir: common::TestDir,
    pool: Arc<db::DbPool>,
    service: ContactsService,
    repo: Arc<ContactsRepository>,
}

fn setup() -> Env {
    let dir = common::TestDir::new();
    let access = db::DbAccess::plaintext(db::get_db_path(dir.path().to_str().unwrap()));
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let repo = Arc::new(ContactsRepository::new(pool.clone(), writer));
    Env {
        _dir: dir,
        pool,
        service: ContactsService::new(repo.clone()),
        repo,
    }
}

fn sql(env: &Env, statement: &str) {
    let mut conn = env.pool.get().unwrap();
    diesel::sql_query(statement).execute(&mut conn).unwrap();
}

fn account(env: &Env, id: &str) {
    sql(env, &format!(
        "INSERT INTO accounts (id, name, account_type, currency, is_default, is_active, created_at, updated_at, is_archived, tracking_mode)
         VALUES ('{id}', 'Compte {id}', 'CASH', 'EUR', 0, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 0, 'TRANSACTIONS')"
    ));
}

fn activity(
    env: &Env,
    id: &str,
    account_id: &str,
    kind: &str,
    amount: &str,
    day: &str,
    note: &str,
) {
    sql(env, &format!(
        "INSERT INTO activities (id,account_id,activity_type,status,activity_date,amount,currency,notes,source_system,is_user_modified,needs_review,created_at,updated_at)
         VALUES ('{id}','{account_id}','{kind}','POSTED','{day}T00:00:00.000Z','{amount}','EUR','{note}','MANUAL',0,0,'2026-01-01','2026-01-01')"
    ));
}

fn counterparty(
    id: &str,
    name: Option<&str>,
    iban: Option<&str>,
    kind: Option<&str>,
) -> OperationCounterparty {
    OperationCounterparty {
        activity_id: id.into(),
        counterparty_name: name.map(str::to_string),
        counterparty_iban: iban.map(str::to_string),
        operation_kind: kind.map(str::to_string),
        contact_id: None,
    }
}

fn person(first: &str, last: &str) -> NewContact {
    NewContact {
        card: ContactInput {
            kind: "person".into(),
            first_name: Some(first.into()),
            last_name: Some(last.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn iban(value: &str) -> NewContactIdentifier {
    NewContactIdentifier {
        kind: "iban".into(),
        value: value.into(),
        ..Default::default()
    }
}

fn name(value: &str) -> NewContactIdentifier {
    NewContactIdentifier {
        kind: "name".into(),
        value: value.into(),
        ..Default::default()
    }
}

/// Two operations with the same person under two printed forms and a second account, and an unknown one.
fn seed_operations(env: &Env) {
    account(env, "acc1");
    activity(
        env,
        "a1",
        "acc1",
        "WITHDRAWAL",
        "50",
        "2026-09-03",
        "VIR Jean Dupont",
    );
    activity(
        env,
        "a2",
        "acc1",
        "DEPOSIT",
        "20",
        "2026-09-10",
        "VIR DUPONT JEAN",
    );
    activity(
        env,
        "a3",
        "acc1",
        "WITHDRAWAL",
        "9.90",
        "2026-09-12",
        "PRLV Netflix",
    );
    activity(
        env,
        "a4",
        "acc1",
        "WITHDRAWAL",
        "3",
        "2026-09-13",
        "CARTE Boulangerie",
    );
}

async fn record_all(env: &Env) {
    env.service
        .record(vec![
            counterparty("a1", Some("Jean Dupont"), Some(IBAN_A), Some("transfer")),
            counterparty("a2", Some("DUPONT JEAN"), None, Some("transfer")),
            counterparty("a3", Some("Netflix"), None, Some("direct_debit")),
            counterparty("a4", Some("Boulangerie"), None, Some("card")),
        ])
        .await
        .unwrap();
}

#[tokio::test]
async fn migrations_create_the_contact_tables() {
    let env = setup();
    assert!(env.repo.list_contacts().unwrap().is_empty());
    assert!(env.repo.list_operations().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_counterparties_wait_to_be_identified_and_shops_stay_out_of_the_list() {
    let env = setup();
    seed_operations(&env);
    record_all(&env).await;

    let todo = env.service.unresolved(false).unwrap();
    let names: Vec<_> = todo.iter().map(|u| u.name.clone().unwrap()).collect();
    // the IBAN-keyed one, the two printed forms of the same name merge into one group when they carry no IBAN
    assert!(names.contains(&"Jean Dupont".to_string()));
    assert!(names.contains(&"Netflix".to_string()));
    assert!(
        !names.contains(&"Boulangerie".to_string()),
        "card payments are shops"
    );
    assert_eq!(env.service.unresolved(true).unwrap().len(), todo.len() + 1);
}

#[tokio::test]
async fn a_contact_with_several_accounts_recognises_all_its_operations() {
    let env = setup();
    seed_operations(&env);
    record_all(&env).await;

    let mut new = person("Jean", "Dupont");
    new.identifiers = vec![iban(IBAN_A), iban(IBAN_B), name("M DUPONT Jean")];
    let created = env.service.create(new).await.unwrap();

    assert_eq!(created.summary.contact.display_name, "Jean Dupont");
    assert_eq!(created.summary.identifiers.len(), 3);
    // a1 by IBAN, a2 by name in another order
    assert_eq!(created.summary.operation_count, 2);
    assert_eq!(created.summary.total_out.to_string(), "50");
    assert_eq!(created.summary.total_in.to_string(), "20");
    assert_eq!(created.summary.last_date.as_deref(), Some("2026-09-10"));
    assert_eq!(created.operations[0].activity_id, "a2");

    // an operation imported later is matched on arrival
    activity(&env, "a5", "acc1", "WITHDRAWAL", "7", "2026-10-01", "VIR");
    env.service
        .record(vec![counterparty(
            "a5",
            None,
            Some("de89 3704 0044 0532 0130 00"),
            None,
        )])
        .await
        .unwrap();
    assert_eq!(
        env.service
            .get(&created.summary.contact.id)
            .unwrap()
            .summary
            .operation_count,
        3
    );
    assert!(env
        .service
        .unresolved(true)
        .unwrap()
        .iter()
        .all(|u| u.name.as_deref() != Some("Jean Dupont")));
}

#[tokio::test]
async fn an_iban_or_name_belongs_to_one_contact_and_an_invalid_iban_is_refused() {
    let env = setup();
    let mut first = person("Jean", "Dupont");
    first.identifiers = vec![iban(IBAN_A)];
    env.service.create(first).await.unwrap();

    let mut second = person("Marie", "Martin");
    second.identifiers = vec![iban(IBAN_A)];
    assert!(env
        .service
        .create(second)
        .await
        .unwrap_err()
        .to_string()
        .contains("Jean Dupont"));

    let mut third = person("Marie", "Martin");
    third.identifiers = vec![iban("FR77 0000 0000 0000 0000 0000 000")];
    assert!(env.service.create(third).await.is_err());
    assert!(
        env.service.list().unwrap().len() == 1,
        "a refused contact leaves nothing behind"
    );

    let mut nameless = person("", "");
    nameless.card.first_name = None;
    nameless.card.last_name = None;
    assert!(env.service.create(nameless).await.is_err());
}

#[tokio::test]
async fn a_contact_can_be_tied_to_imported_accounts_and_known_ibans_are_recognised() {
    let env = setup();
    account(&env, "acc1");
    account(&env, "acc2");
    // acc2 is one of the user's own accounts, with its IBAN read from a statement
    sql(&env, &format!(
        "INSERT INTO account_identities (account_id, iban, edited_by_user, updated_at) VALUES ('acc2', '{}', 0, '2026-01-01')",
        IBAN_B.replace(' ', "")
    ));
    activity(
        &env,
        "a1",
        "acc1",
        "WITHDRAWAL",
        "100",
        "2026-09-03",
        "VIR Epargne",
    );
    env.service
        .record(vec![counterparty(
            "a1",
            Some("Epargne"),
            Some(IBAN_B),
            None,
        )])
        .await
        .unwrap();

    let todo = env.service.unresolved(true).unwrap();
    assert_eq!(todo[0].own_account_id.as_deref(), Some("acc2"));

    let mut me = person("Jean", "Dupont");
    me.card.kind = "self".into();
    me.linked_account_ids = vec!["acc1".into(), "acc2".into()];
    let created = env.service.create(me).await.unwrap();
    assert_eq!(created.summary.linked_account_ids.len(), 2);
    // the operation towards acc2 is now with this contact, through the IBAN of the linked account
    assert_eq!(created.summary.operation_count, 1);

    assert!(env
        .service
        .link_account(&created.summary.contact.id, "nope")
        .await
        .is_err());
    let unlinked = env
        .service
        .unlink_account(&created.summary.contact.id, "acc2")
        .await
        .unwrap();
    assert_eq!(unlinked.summary.operation_count, 0);
}

#[tokio::test]
async fn adding_and_removing_an_identifier_relinks_operations() {
    let env = setup();
    seed_operations(&env);
    record_all(&env).await;
    let created = env.service.create(person("Net", "Flix")).await.unwrap();
    let id = created.summary.contact.id.clone();
    assert_eq!(created.summary.operation_count, 0);

    let with = env
        .service
        .add_identifier(&id, name("NETFLIX"))
        .await
        .unwrap();
    assert_eq!(with.summary.operation_count, 1);
    // adding the same thing again changes nothing
    let again = env
        .service
        .add_identifier(&id, name("netflix"))
        .await
        .unwrap();
    assert_eq!(again.summary.identifiers.len(), 1);

    let without = env
        .service
        .remove_identifier(&with.summary.identifiers[0].id)
        .await
        .unwrap();
    assert_eq!(without.summary.operation_count, 0);
}

#[tokio::test]
async fn merging_moves_identifiers_accounts_and_operations() {
    let env = setup();
    seed_operations(&env);
    record_all(&env).await;
    account(&env, "acc2");

    let mut a = person("Jean", "Dupont");
    a.identifiers = vec![iban(IBAN_A)];
    a.linked_account_ids = vec!["acc2".into()];
    let a = env.service.create(a).await.unwrap();
    let mut b = person("J", "Dupont");
    b.identifiers = vec![name("Netflix")];
    b.card.phone = Some("0102030405".into());
    let b = env.service.create(b).await.unwrap();

    let merged = env
        .service
        .merge(&a.summary.contact.id, &b.summary.contact.id)
        .await
        .unwrap();
    assert_eq!(merged.summary.identifiers.len(), 2);
    assert_eq!(merged.summary.linked_account_ids, vec!["acc2".to_string()]);
    assert_eq!(merged.summary.operation_count, 2);
    assert_eq!(merged.summary.contact.phone.as_deref(), Some("0102030405"));
    assert_eq!(env.service.list().unwrap().len(), 1);
    assert!(env
        .service
        .merge(&b.summary.contact.id, &b.summary.contact.id)
        .await
        .is_err());
}

#[tokio::test]
async fn deleting_a_contact_keeps_the_operations_and_a_removed_operation_drops_its_counterparty() {
    let env = setup();
    seed_operations(&env);
    record_all(&env).await;
    let mut new = person("Jean", "Dupont");
    new.identifiers = vec![iban(IBAN_A)];
    let created = env.service.create(new).await.unwrap();
    assert_eq!(created.summary.operation_count, 1);

    env.service
        .delete(&created.summary.contact.id)
        .await
        .unwrap();
    assert!(env.service.list().unwrap().is_empty());
    let kept = env.repo.list_operations().unwrap();
    assert_eq!(kept.len(), 4);
    assert!(kept.iter().all(|o| o.counterparty.contact_id.is_none()));
    assert!(env.service.get(&created.summary.contact.id).is_err());

    // deleting an activity removes its counterparty row
    sql(&env, "DELETE FROM activities WHERE id = 'a1'");
    assert_eq!(env.repo.list_operations().unwrap().len(), 3);
}

#[tokio::test]
async fn deleting_an_account_drops_its_link_to_the_contact() {
    let env = setup();
    account(&env, "acc1");
    let mut new = person("Jean", "Dupont");
    new.linked_account_ids = vec!["acc1".into()];
    let created = env.service.create(new).await.unwrap();
    sql(&env, "DELETE FROM accounts WHERE id = 'acc1'");
    assert!(env
        .service
        .get(&created.summary.contact.id)
        .unwrap()
        .summary
        .linked_account_ids
        .is_empty());
}

#[tokio::test]
async fn a_reference_taken_for_an_iban_is_not_kept() {
    let env = setup();
    account(&env, "acc1");
    activity(
        &env,
        "a1",
        "acc1",
        "WITHDRAWAL",
        "10",
        "2026-09-03",
        "VIR Kraken",
    );
    env.service
        .record(vec![counterparty(
            "a1",
            Some("Kraken"),
            Some("Kraken AA97N84GHBIAMLLI"),
            None,
        )])
        .await
        .unwrap();
    let stored = env.repo.list_operations().unwrap();
    assert_eq!(stored[0].counterparty.counterparty_iban, None);
    let todo = env.service.unresolved(true).unwrap();
    assert_eq!(todo[0].kind, "name");
}

#[tokio::test]
async fn spending_by_beneficiary_groups_by_contact_and_leaves_own_transfers_out() {
    let env = setup();
    seed_operations(&env);
    // a transfer to another own account: one leg is linked, it must not count as spending
    activity(
        &env,
        "a5",
        "acc1",
        "TRANSFER_OUT",
        "200",
        "2026-09-14",
        "VIR Livret",
    );
    sql(
        &env,
        "UPDATE activities SET source_group_id = 'g1' WHERE id = 'a5'",
    );
    record_all(&env).await;
    env.service
        .record(vec![counterparty(
            "a5",
            Some("Livret"),
            None,
            Some("transfer"),
        )])
        .await
        .unwrap();
    let mut new = person("Jean", "Dupont");
    new.identifiers = vec![iban(IBAN_A), name("DUPONT JEAN")];
    let jean = env.service.create(new).await.unwrap();

    let rows = env.service.spending_by_beneficiary(None, None).unwrap();
    let names: Vec<_> = rows.iter().map(|r| r.name.clone()).collect();
    assert!(!names.contains(&"Livret".to_string()), "{names:?}");
    // biggest spending first: Jean Dupont (50 out, 20 in), then Netflix, then the bakery
    assert_eq!(
        rows[0].contact_id.as_deref(),
        Some(jean.summary.contact.id.as_str())
    );
    assert_eq!(rows[0].count, 2);
    assert_eq!(rows[0].total_out.to_string(), "50");
    assert_eq!(rows[0].total_in.to_string(), "20");
    assert_eq!(rows[1].name, "Netflix");

    // a period keeps only what happened in it
    let rows = env
        .service
        .spending_by_beneficiary(Some("2026-09-11"), Some("2026-09-30"))
        .unwrap();
    assert!(rows.iter().all(|r| r.name != "Jean Dupont"));
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn a_joint_account_is_split_between_its_holders_and_each_sees_their_part() {
    let env = setup();
    seed_operations(&env);
    record_all(&env).await;
    let marie = env.service.create(person("Marie", "Martin")).await.unwrap();
    let paul = env.service.create(person("Paul", "Martin")).await.unwrap();
    let (marie_id, paul_id) = (
        marie.summary.contact.id.clone(),
        paul.summary.contact.id.clone(),
    );
    // a lone holder owns all of the account
    let marie = env.service.link_account(&marie_id, "acc1").await.unwrap();
    assert_eq!(
        marie.summary.account_shares[0].share_percent.to_string(),
        "100"
    );
    // a second holder: the account is split evenly
    let paul = env.service.link_account(&paul_id, "acc1").await.unwrap();
    assert_eq!(
        paul.summary.account_shares[0].share_percent.to_string(),
        "50"
    );
    let marie = env.service.get(&marie_id).unwrap();
    assert_eq!(
        marie.summary.account_shares[0].share_percent.to_string(),
        "50"
    );
    // shares are typed by the person; out of range is refused
    assert!(env
        .service
        .set_account_share(&marie_id, "acc1", rust_decimal::Decimal::from(101))
        .await
        .is_err());
    env.service
        .set_account_share(&marie_id, "acc1", rust_decimal::Decimal::from(70))
        .await
        .unwrap();
    // 70 + 50 does not add up to 100: said so
    sql(
        &env,
        "INSERT INTO statement_documents (id,account_id,file_name,stored_name,content_type,size_bytes,sha256,document_kind,period_start,period_end,status,created_at,content,closing_balance)
         VALUES ('d1','acc1','r.pdf','r.pdf','application/pdf',1,'h1','statement','2026-09-01','2026-09-30','VALIDATED','2026-10-01',x'00','1000.00')",
    );
    let overview = env.service.person_overview(&marie_id, None, None).unwrap();
    assert_eq!(
        overview.unbalanced_accounts,
        vec!["Compte acc1".to_string()]
    );
    let account = &overview.accounts[0];
    assert!(account.joint);
    assert_eq!(account.co_holders[0].name, "Paul Martin");
    // 70 % of the 1 000 € of the latest statement
    assert_eq!(account.balance.unwrap().to_string(), "700.00");
    // 70 % of the 20 € that came in and of the 62.90 € that went out
    assert_eq!(account.income.to_string(), "14.00");
    assert_eq!(account.spending.to_string(), "44.03");
    // unlinking Paul gives the whole account back to Marie
    env.service.unlink_account(&paul_id, "acc1").await.unwrap();
    assert_eq!(
        env.service.get(&marie_id).unwrap().summary.account_shares[0]
            .share_percent
            .to_string(),
        "100"
    );
}

#[tokio::test]
async fn a_transfer_to_an_own_account_that_wealthfolio_does_not_have_is_reported_as_missing() {
    let env = setup();
    seed_operations(&env);
    account(&env, "acc2");
    activity(
        &env,
        "t1",
        "acc1",
        "TRANSFER_OUT",
        "100",
        "2026-09-20",
        "VIR Livret A",
    );
    activity(
        &env,
        "t2",
        "acc1",
        "TRANSFER_OUT",
        "50",
        "2026-09-25",
        "VIR Livret A",
    );
    let mut me = person("Moi", "Même");
    me.card.kind = "self".into();
    me.identifiers = vec![NewContactIdentifier {
        label: Some("Livret A".into()),
        ..iban(IBAN_B)
    }];
    let me = env.service.create(me).await.unwrap();
    env.service
        .record(vec![
            counterparty("t1", Some("Livret A"), Some(IBAN_B), Some("transfer")),
            counterparty("t2", Some("Livret A"), Some(IBAN_B), Some("transfer")),
        ])
        .await
        .unwrap();
    let missing = env.service.missing_own_accounts().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].contact_id, me.summary.contact.id);
    assert_eq!(missing[0].label.as_deref(), Some("Livret A"));
    assert_eq!(missing[0].count, 2);
    assert_eq!(missing[0].total_out.to_string(), "150");
    assert_eq!(missing[0].account_ids, vec!["acc1".to_string()]);
    // they are transfers: neither spending nor income
    let rows = env.service.spending_by_beneficiary(None, None).unwrap();
    assert!(rows.iter().all(|r| r.name != "Livret A"));
    // once the account exists in Wealthfolio with this IBAN, nothing is missing any more
    sql(
        &env,
        &format!(
            "INSERT INTO account_identities (account_id, iban, updated_at) VALUES ('acc2', '{IBAN_B}', '2026-10-01')"
        ),
    );
    assert!(env.service.missing_own_accounts().unwrap().is_empty());
}
