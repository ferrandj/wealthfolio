-- Joint accounts: each person holds a share of the account (in percent, 100 for an account with a single holder).
ALTER TABLE contact_linked_accounts ADD COLUMN share_percent TEXT NOT NULL DEFAULT '100';
