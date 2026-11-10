//! Rates against a real, migrated SQLite database: defaults appear on first use, an account rate is created and
//! shows its net value, and deleting the account takes its rate away.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
mod common;

use diesel::prelude::*;
use rust_decimal::Decimal;
use std::sync::Arc;

use wealthfolio_core::rates::{RateInput, RatesService, RatesServiceTrait};
use wealthfolio_storage_sqlite::db;
use wealthfolio_storage_sqlite::rates::RatesRepository;

fn setup() -> (common::TestDir, Arc<db::DbPool>, RatesService) {
    let dir = common::TestDir::new();
    let access = db::DbAccess::plaintext(db::get_db_path(dir.path().to_str().unwrap()));
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let writer = db::write_actor::spawn_writer((*pool).clone()).unwrap();
    let service = RatesService::new(Arc::new(RatesRepository::new(pool.clone(), writer)));
    (dir, pool, service)
}

fn account(pool: &db::DbPool, id: &str) {
    let mut conn = pool.get().unwrap();
    diesel::sql_query(format!(
        "INSERT INTO accounts (id, name, account_type, currency, is_default, is_active, created_at, updated_at, is_archived, tracking_mode)
         VALUES ('{id}', 'Compte {id}', 'CASH', 'EUR', 0, 1, '2026-01-01 00:00:00', '2026-01-01 00:00:00', 0, 'TRANSACTIONS')"
    ))
    .execute(&mut conn)
    .unwrap();
}

#[tokio::test]
async fn the_defaults_appear_on_first_use_and_can_be_edited() {
    let (_dir, _pool, service) = setup();
    let rates = service.list().await.unwrap();
    let ids: Vec<_> = rates.iter().map(|r| r.rate.id.as_str()).collect();
    assert!(ids.contains(&"regulated.livret_a"));
    assert!(ids.contains(&"tax.flat_tax"));
    let edited = service
        .save(
            "regulated.livret_a",
            RateInput {
                rate: Some(Decimal::new(25, 1)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.rate.rate, Decimal::new(25, 1));
    assert_eq!(edited.rate.source.as_deref(), Some("Saisi à la main"));
    // a regulated rate can be changed, not removed
    assert!(service.delete("regulated.livret_a").await.is_err());
}

#[tokio::test]
async fn an_account_rate_shows_its_net_value_and_follows_the_account() {
    let (_dir, pool, service) = setup();
    account(&pool, "acc1");
    let view = service
        .save(
            "account.acc1",
            RateInput {
                rate: Some(Decimal::from(2)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(view.gross_rate, Decimal::from(2));
    assert_eq!(view.net_rate, Decimal::new(1372, 3));
    // an account that does not exist has no rate
    assert!(service
        .save(
            "account.nope",
            RateInput {
                rate: Some(Decimal::ONE),
                ..Default::default()
            }
        )
        .await
        .is_err());
    // a rate out of range is refused
    assert!(service
        .save(
            "account.acc1",
            RateInput {
                rate: Some(Decimal::from(150)),
                ..Default::default()
            }
        )
        .await
        .is_err());
    {
        let mut conn = pool.get().unwrap();
        diesel::sql_query("PRAGMA foreign_keys = ON")
            .execute(&mut conn)
            .unwrap();
        diesel::sql_query("DELETE FROM accounts WHERE id = 'acc1'")
            .execute(&mut conn)
            .unwrap();
    }
    let rates = service.list().await.unwrap();
    assert!(rates.iter().all(|r| r.rate.id != "account.acc1"));
}

#[tokio::test]
async fn an_account_follows_the_tax_of_its_investment_type_and_the_review_comes_every_six_months() {
    let (_dir, pool, service) = setup();
    account(&pool, "pea");
    let view = service
        .save(
            "account.pea",
            RateInput {
                rate: Some(Decimal::from(2)),
                tax_type: Some("tax.type.pea_after_5y".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(view.net_rate, Decimal::new(1628, 3));
    // a tax type that is not in the table is refused, an empty one goes back to the flat tax
    assert!(service
        .save(
            "account.pea",
            RateInput {
                tax_type: Some("tax.type.nope".into()),
                ..Default::default()
            }
        )
        .await
        .is_err());
    let back = service
        .save(
            "account.pea",
            RateInput {
                tax_type: Some(String::new()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(back.net_rate, Decimal::new(1372, 3));
    // the tables created today are confirmed today: nothing to confirm until the next revision
    let day = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let today = chrono::Utc::now().date_naive();
    assert!(!service.review(today).await.unwrap().due);
    // after the next revision date the person is asked again, and confirming clears it
    let later = day(2099, 2, 2);
    let review = service.review(later).await.unwrap();
    assert!(review.due);
    assert_eq!(review.last_revision, day(2099, 2, 1));
    assert!(!service.confirm(today).await.unwrap().due);
}
