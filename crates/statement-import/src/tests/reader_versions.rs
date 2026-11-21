//! Each adapter has the fingerprint of its own reading code.

use crate::adapters::registry::{reader_version, reader_version_of, ADAPTERS, READER_VERSIONS};

#[test]
fn test_every_adapter_has_its_own_reader_version() {
    for adapter in ADAPTERS {
        assert!(
            READER_VERSIONS.iter().any(|(id, _)| *id == adapter.id()),
            "no reader version for {}",
            adapter.id()
        );
        assert_eq!(reader_version_of(Some(adapter.id())).len(), 8);
    }
    assert_eq!(reader_version_of(None), reader_version());
    assert_eq!(reader_version_of(Some("nobody.knows")), reader_version());
}

#[test]
fn test_banks_that_share_no_code_have_different_versions() {
    // Revolut's reader uses neither BoursoBank's nor Oradéa's files: a change to theirs leaves it as it is
    let revolut = reader_version_of(Some("revolut.statement_csv"));
    assert_ne!(revolut, reader_version_of(Some("oradea.tax_form_pdf")));
}
