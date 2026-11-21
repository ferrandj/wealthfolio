ALTER TABLE statement_documents DROP COLUMN reading_fingerprint;
ALTER TABLE statement_documents ADD COLUMN content BLOB NOT NULL DEFAULT X'';
UPDATE statement_documents SET content = (
    SELECT c.content FROM statement_document_contents c WHERE c.document_id = statement_documents.id
) WHERE id IN (SELECT document_id FROM statement_document_contents);
DROP TABLE statement_document_contents;
