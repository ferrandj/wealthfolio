-- Who owns a bank account and how to reach it (name, address, IBAN, BIC, RIB), read from the account's documents
-- or typed by the user. One row per account.
CREATE TABLE account_identities (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    institution TEXT,
    holder_name TEXT,
    holder_address TEXT,
    iban TEXT,
    bic TEXT,
    bank_code TEXT,
    branch_code TEXT,
    account_number TEXT,
    rib_key TEXT,
    -- once the user has typed a value, documents only fill what is still empty
    edited_by_user INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_account_identities_iban ON account_identities (iban);

-- The originals of the documents dropped for import (statements, fee summaries, loan papers...), kept as received.
-- The bytes live in the database so that backups, encryption and restores cover them like everything else.
CREATE TABLE statement_documents (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    file_name TEXT NOT NULL,
    stored_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    sha256 TEXT NOT NULL,
    institution TEXT,
    adapter_id TEXT,
    document_kind TEXT,
    account_type TEXT,
    period_start TEXT,
    period_end TEXT,
    holder_name TEXT,
    iban TEXT,
    reader_version TEXT,
    imported_count INTEGER NOT NULL DEFAULT 0,
    duplicate_count INTEGER NOT NULL DEFAULT 0,
    warnings TEXT NOT NULL DEFAULT '[]',
    import_run_id TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    content BLOB NOT NULL
);
-- The same bytes are never stored twice.
CREATE UNIQUE INDEX idx_statement_documents_sha256 ON statement_documents (sha256);
CREATE INDEX idx_statement_documents_account ON statement_documents (account_id);
