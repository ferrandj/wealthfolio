-- The bytes of the kept originals move to a table of their own: reading the list of documents (or one document's
-- fields) no longer walks through every original stored in the same rows.
CREATE TABLE statement_document_contents (
    document_id TEXT PRIMARY KEY NOT NULL REFERENCES statement_documents(id) ON DELETE CASCADE,
    content BLOB NOT NULL
);
INSERT INTO statement_document_contents (document_id, content) SELECT id, content FROM statement_documents;
ALTER TABLE statement_documents DROP COLUMN content;

-- What the readers found in a document when its operations were brought in: read again the same, nothing is redone.
ALTER TABLE statement_documents ADD COLUMN reading_fingerprint TEXT;
