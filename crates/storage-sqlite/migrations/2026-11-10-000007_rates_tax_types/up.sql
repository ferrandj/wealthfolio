-- The tax an account's interest goes through depends on the kind of investment: it points at a row of the tax table.
ALTER TABLE rates ADD COLUMN tax_type TEXT;

-- Small facts about the rate tables (when the person last confirmed them).
CREATE TABLE rates_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
