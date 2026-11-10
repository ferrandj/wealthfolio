//! Readers for the documents banks hand out (statements, savings letters, loan offers, OFX/QIF/CSV exports).
//!
//! Every adapter recognises one kind of document from one institution and turns it into [`ParsedStatement`]s.
//! The registry scores a dropped file against every adapter and either picks the clear winner or tells the caller
//! to ask the user. This crate does no I/O and knows nothing about Wealthfolio's database.

pub mod account_kind;
pub mod adapters;
pub mod analysis;
pub mod naming;
pub mod pdf;
pub mod py;

pub use adapters::registry::{
    catalog, get_adapter, identify, parse_with, reader_version, Candidate, Identification,
};
pub use adapters::{
    Adapter, Detection, HoldingData, ParsedStatement, SourceDocument, TransactionData,
};
pub use analysis::{
    analyze, sha256_hex, supported_documents, Analysis, StatementPreview, SupportedAdapter,
    SupportedInstitution, TransactionPreview,
};
pub use naming::{account_type_label, document_kind_label, standard_file_name};

#[cfg(test)]
mod testkit;
#[cfg(test)]
mod tests;
