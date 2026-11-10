UPDATE statement_documents SET status = 'IMPORTED' WHERE status IN ('PENDING', 'VALIDATED');
ALTER TABLE statement_documents DROP COLUMN validated_at;
ALTER TABLE statement_documents DROP COLUMN to_review_count;
ALTER TABLE statement_documents DROP COLUMN review_reason;
