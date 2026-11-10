-- Interest and tax rates the person keeps next to their accounts: the regulated savings rates, the tax table,
-- and the rate of each account.
CREATE TABLE rates (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,                 -- regulated | tax | account
    label TEXT NOT NULL,
    rate TEXT NOT NULL,                 -- percent, as a decimal string
    gross INTEGER NOT NULL DEFAULT 1,   -- account rates: the rate entered is before tax
    tax_exempt INTEGER NOT NULL DEFAULT 0,
    account_id TEXT REFERENCES accounts(id) ON DELETE CASCADE,
    source TEXT,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_rates_account ON rates(account_id) WHERE account_id IS NOT NULL;
