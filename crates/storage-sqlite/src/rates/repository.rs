use async_trait::async_trait;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use wealthfolio_core::rates::{Rate, RatesRepositoryTrait};
use wealthfolio_core::Result;

use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;
use crate::schema::{accounts, rates};

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug, Clone)]
#[diesel(table_name = crate::schema::rates)]
#[diesel(treat_none_as_null = true)]
struct RateDB {
    id: String,
    kind: String,
    label: String,
    rate: String,
    gross: bool,
    tax_exempt: bool,
    account_id: Option<String>,
    source: Option<String>,
    updated_at: String,
}

impl From<RateDB> for Rate {
    fn from(db: RateDB) -> Self {
        Self {
            id: db.id,
            kind: db.kind,
            label: db.label,
            rate: db.rate.parse().unwrap_or_default(),
            gross: db.gross,
            tax_exempt: db.tax_exempt,
            account_id: db.account_id,
            source: db.source,
            updated_at: db.updated_at,
        }
    }
}

impl From<Rate> for RateDB {
    fn from(rate: Rate) -> Self {
        Self {
            id: rate.id,
            kind: rate.kind,
            label: rate.label,
            rate: rate.rate.normalize().to_string(),
            gross: rate.gross,
            tax_exempt: rate.tax_exempt,
            account_id: rate.account_id,
            source: rate.source,
            updated_at: rate.updated_at,
        }
    }
}

pub struct RatesRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl RatesRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

#[async_trait]
impl RatesRepositoryTrait for RatesRepository {
    fn list(&self) -> Result<Vec<Rate>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = rates::table
            .select(RateDB::as_select())
            .load::<RateDB>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows.into_iter().map(Rate::from).collect())
    }

    fn get(&self, id: &str) -> Result<Option<Rate>> {
        let mut conn = get_connection(&self.pool)?;
        let row = rates::table
            .find(id)
            .select(RateDB::as_select())
            .first::<RateDB>(&mut conn)
            .optional()
            .map_err(StorageError::from)?;
        Ok(row.map(Rate::from))
    }

    async fn save(&self, rate: Rate) -> Result<Rate> {
        let row = RateDB::from(rate.clone());
        self.writer
            .exec(move |conn| -> Result<Rate> {
                diesel::replace_into(rates::table)
                    .values(&row)
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(rate)
            })
            .await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.writer
            .exec(move |conn| -> Result<()> {
                diesel::delete(rates::table.find(&id))
                    .execute(conn)
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn account_exists(&self, account_id: &str) -> Result<bool> {
        let mut conn = get_connection(&self.pool)?;
        let n: i64 = accounts::table
            .find(account_id)
            .count()
            .get_result(&mut conn)
            .map_err(StorageError::from)?;
        Ok(n > 0)
    }
}
