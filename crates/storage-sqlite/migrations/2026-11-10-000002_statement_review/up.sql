-- A document is validated once its operations have been checked (balance reconciles, categories confirmed);
-- until then it waits for review, with the reason why.
ALTER TABLE statement_documents ADD COLUMN review_reason TEXT;
ALTER TABLE statement_documents ADD COLUMN to_review_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE statement_documents ADD COLUMN validated_at TEXT;
-- documents imported before review existed have not been validated by anyone
UPDATE statement_documents SET status = 'PENDING' WHERE status = 'IMPORTED';
