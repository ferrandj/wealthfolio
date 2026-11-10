-- Opening and closing balances of a statement, to check that consecutive statements follow each other.
ALTER TABLE statement_documents ADD COLUMN opening_balance TEXT;
ALTER TABLE statement_documents ADD COLUMN closing_balance TEXT;

-- A month without a statement that the person said is normal (account not open yet, nothing happened...).
CREATE TABLE statement_gap_marks (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    month TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (account_id, month)
);
