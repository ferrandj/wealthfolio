//! The spending side of the statements feature: the categories imported bank operations get, and the spending
//! tracking of the accounts they land in. `wealthfolio-core` defines the port and this crate implements it,
//! because core cannot depend on spending.

use std::sync::Arc;

use async_trait::async_trait;
use wealthfolio_core::errors::{Error, Result};
use wealthfolio_core::statements::{CategoryAssignerTrait, CategoryAssignment};

use crate::activity_assignments::{
    ActivityTaxonomyAssignmentRepositoryTrait, NewActivityTaxonomyAssignment,
};
use crate::settings::{SpendingSettingsService, SpendingSettingsUpdate};

const SOURCE_IMPORT: &str = "import";

fn unexpected(e: anyhow::Error) -> Error {
    Error::Unexpected(e.to_string())
}

pub struct StatementCategoryAssigner {
    settings: Arc<SpendingSettingsService>,
    assignments: Arc<dyn ActivityTaxonomyAssignmentRepositoryTrait>,
}

impl StatementCategoryAssigner {
    pub fn new(
        settings: Arc<SpendingSettingsService>,
        assignments: Arc<dyn ActivityTaxonomyAssignmentRepositoryTrait>,
    ) -> Self {
        Self {
            settings,
            assignments,
        }
    }
}

#[async_trait]
impl CategoryAssignerTrait for StatementCategoryAssigner {
    async fn track_account(&self, account_id: &str) -> Result<()> {
        let current = self.settings.get().await.map_err(unexpected)?;
        if current.enabled && current.account_ids.iter().any(|id| id == account_id) {
            return Ok(());
        }
        let mut account_ids = current.account_ids.clone();
        if !account_ids.iter().any(|id| id == account_id) {
            account_ids.push(account_id.to_string());
        }
        self.settings
            .update(SpendingSettingsUpdate {
                enabled: Some(true),
                account_ids: Some(account_ids),
                excluded_category_ids: None,
            })
            .await
            .map_err(unexpected)?;
        Ok(())
    }

    async fn assign(&self, items: Vec<CategoryAssignment>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = items.iter().map(|i| i.activity_id.clone()).collect();
        let existing = self
            .assignments
            .list_for_activities(&ids)
            .await
            .map_err(unexpected)?;
        let news: Vec<NewActivityTaxonomyAssignment> = items
            .into_iter()
            // a guess never replaces what a rule or a person already decided
            .filter(|item| {
                item.source != SOURCE_IMPORT
                    || !existing.iter().any(|e| {
                        e.activity_id == item.activity_id && e.taxonomy_id == item.taxonomy_id
                    })
            })
            .map(|item| NewActivityTaxonomyAssignment {
                id: None,
                activity_id: item.activity_id,
                taxonomy_id: item.taxonomy_id,
                category_id: item.category_id,
                weight: 10_000,
                source: item.source,
            })
            .collect();
        if news.is_empty() {
            return Ok(());
        }
        self.assignments
            .assign_many_single_select(news)
            .await
            .map_err(unexpected)?;
        Ok(())
    }

    async fn current(&self, activity_ids: &[String]) -> Result<Vec<CategoryAssignment>> {
        if activity_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = self
            .assignments
            .list_for_activities(activity_ids)
            .await
            .map_err(unexpected)?;
        Ok(rows
            .into_iter()
            .map(|r| CategoryAssignment {
                activity_id: r.activity_id,
                taxonomy_id: r.taxonomy_id,
                category_id: r.category_id,
                source: r.source,
            })
            .collect())
    }
}
