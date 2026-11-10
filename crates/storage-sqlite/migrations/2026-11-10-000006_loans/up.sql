-- What the bank's loan papers (contract, early repayment letter, dated table) say, kept per document so that the
-- loan can be rebuilt from them. Removing the document removes its data.
CREATE TABLE loan_documents (
    document_id TEXT PRIMARY KEY NOT NULL REFERENCES statement_documents(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    reference TEXT,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- What the person typed about a loan (the date of instalment n°1 when the papers do not give it).
CREATE TABLE loan_settings (
    loan_key TEXT PRIMARY KEY NOT NULL,
    first_due TEXT,
    updated_at TEXT NOT NULL
);
