//! Bank statements and account documents: dropped files are read by the statement-import adapters, their
//! operations become cash activities, their originals are kept, and the account holder's identity (name,
//! address, IBAN, BIC, RIB) is stored with the account so it can be copied.

mod loans;
mod loans_liabilities;
mod loans_plan;
mod loans_reconcile;
mod statements_coverage;
mod statements_model;
mod statements_review;
mod statements_service;
mod statements_traits;

pub use loans::{
    assemble_loans, document_problems, is_loan_kind, LoanDocumentData, LoanDocumentRef, LoanView,
};
pub use loans_liabilities::{
    changes_of, setup_of, LoanAssetLink, LoanAssetLinked, LoanChange, LoanLiabilities,
    NewLoanAsset, LOAN_REFERENCE_KEY,
};
pub use loans_plan::{
    build_plan, DatedRow, EventData, LoanInput, OfferData, OfferRow, Plan, PlanCheck, PlanRow,
    PlanSummary, Saved, ScheduleData,
};
pub use loans_reconcile::{
    first_due_from_debits, gaps_of, reconcile, BankDebit, LoanAnswers, LoanGap, LoanPayment,
    LoanPayments, StatementPeriod,
};
pub use statements_coverage::{compute_coverage, AccountCoverage, BalanceBreak, MissingMonth};
pub use statements_model::{
    AccountIdentity, AccountIdentityInput, AnalyzedStatement, CategoryAssignment, DocumentReview,
    ImportMode, MatchedAccount, NewStatementDocument, OutdatedDocuments, RevalidationOutcome,
    RevalidationResult, ReviewDecision, ReviewedOperation, StatementAnalysis, StatementDocument,
    StatementImportOutcome, StatementImportRequest, StatementImportResult, StatementOverlap,
    StatementOverride, StatementReview, DOCUMENT_STATUS_PENDING, DOCUMENT_STATUS_STORED,
    DOCUMENT_STATUS_VALIDATED,
};
pub use statements_review::{
    balance_gap, is_generic_category, is_own_transfer_category, logical_order,
    suggestion_replaces_kept, ReviewReason,
};
pub use statements_service::{StatementService, MAX_DOCUMENT_BYTES, OPENING_BALANCE_NOTE};
pub use statements_traits::{
    CategoryAssignerTrait, StatementRepositoryTrait, StatementServiceTrait,
};

pub use wealthfolio_statement_import::{SupportedAdapter, SupportedInstitution};

/// The documents that can be read, by institution.
pub fn supported_documents() -> Vec<SupportedInstitution> {
    wealthfolio_statement_import::supported_documents()
}
