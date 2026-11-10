//! Bank statements and account documents: dropped files are read by the statement-import adapters, their
//! operations become cash activities, their originals are kept, and the account holder's identity (name,
//! address, IBAN, BIC, RIB) is stored with the account so it can be copied.

mod statements_model;
mod statements_service;
mod statements_traits;

pub use statements_model::{
    AccountIdentity, AccountIdentityInput, AnalyzedStatement, ImportMode, MatchedAccount,
    NewStatementDocument, StatementAnalysis, StatementDocument, StatementImportOutcome,
    StatementImportRequest, StatementImportResult, StatementOverride, DOCUMENT_STATUS_IMPORTED,
    DOCUMENT_STATUS_STORED,
};
pub use statements_service::{StatementService, MAX_DOCUMENT_BYTES, OPENING_BALANCE_NOTE};
pub use statements_traits::{StatementRepositoryTrait, StatementServiceTrait};

pub use wealthfolio_statement_import::{SupportedAdapter, SupportedInstitution};

/// The documents that can be read, by institution.
pub fn supported_documents() -> Vec<SupportedInstitution> {
    wealthfolio_statement_import::supported_documents()
}
