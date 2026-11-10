#![allow(dead_code)]
//! What the tests share: a small PDF writer, the synthetic documents and a few helpers. Only compiled for tests.

pub mod fixtures;
pub mod fortuneo;
pub mod hellobank;
pub mod loan;
pub mod pdf;

use crate::adapters::base::{ParsedStatement, SourceDocument};
use crate::adapters::registry::{identify, parse_with};

/// A PDF of rows of (x, text) blocks in millimetres.
pub fn rows_pdf(rows: &[Vec<(f64, &str)>]) -> Vec<u8> {
    let mut pdf = pdf::Pdf::new("");
    let mm = 72.0 / 25.4;
    let mut y = 20.0;
    for row in rows {
        for (x, text) in row {
            pdf.text(x * mm, y * mm, text);
        }
        y += 6.0;
    }
    pdf.bytes()
}

/// What the importer does: pick the adapter by itself, then read.
pub fn parse_document(filename: &str, data: Vec<u8>) -> Vec<ParsedStatement> {
    let doc = SourceDocument::new(filename, data);
    let found = identify(&doc);
    let chosen = found
        .chosen
        .as_ref()
        .unwrap_or_else(|| panic!("{}", found.message));
    parse_with(chosen.adapter.id(), &doc).unwrap()
}

/// The single statement an adapter reads from a document.
pub fn parse_one(adapter: &str, filename: &str, data: Vec<u8>) -> ParsedStatement {
    let doc = SourceDocument::new(filename, data);
    let mut statements = parse_with(adapter, &doc).unwrap();
    assert_eq!(statements.len(), 1);
    statements.remove(0)
}

/// The Windows-1252 byte of a character (for the French CSV exports).
pub fn cp1252_byte(c: char) -> u8 {
    pdf::cp1252(c)
}
