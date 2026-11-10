-- Contacts: the people and organisations money goes to or comes from (beneficiaries, debtors), and the people behind
-- accounts. A contact is recognised in imported operations by the IBANs or printed names it carries, and can be tied
-- to accounts already in Wealthfolio (imported ones included).
CREATE TABLE contacts (
    id TEXT PRIMARY KEY NOT NULL,
    -- person | organization | self | other
    kind TEXT NOT NULL,
    -- none | beneficiary | debtor | both
    role TEXT NOT NULL DEFAULT 'none',
    display_name TEXT NOT NULL,
    civility TEXT,
    last_name TEXT,
    first_name TEXT,
    birth_name TEXT,
    address TEXT,
    phone TEXT,
    email TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- What identifies a contact on the other side of an operation: one of its bank accounts (IBAN) or the name the bank
-- prints. Several per contact; a value belongs to a single contact.
CREATE TABLE contact_identifiers (
    id TEXT PRIMARY KEY NOT NULL,
    contact_id TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    -- iban | name
    kind TEXT NOT NULL,
    value TEXT NOT NULL,
    label TEXT,
    bic TEXT,
    institution TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (kind, value)
);
CREATE INDEX idx_contact_identifiers_contact ON contact_identifiers (contact_id);

-- Accounts that already exist in Wealthfolio and belong to the contact (the holder's own accounts, imported or not).
CREATE TABLE contact_linked_accounts (
    contact_id TEXT NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    PRIMARY KEY (contact_id, account_id)
);
CREATE INDEX idx_contact_linked_accounts_account ON contact_linked_accounts (account_id);

-- Who the other side of an imported operation is, as printed by the bank, and the contact it was matched to.
CREATE TABLE activity_counterparties (
    activity_id TEXT PRIMARY KEY NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    counterparty_name TEXT,
    counterparty_iban TEXT,
    -- the reader's kind of operation (card, transfer, direct debit...)
    operation_kind TEXT,
    contact_id TEXT REFERENCES contacts(id) ON DELETE SET NULL
);
CREATE INDEX idx_activity_counterparties_contact ON activity_counterparties (contact_id);
