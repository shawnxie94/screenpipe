-- Drop the never-used ocr_text_embeddings table. The table was created for a
-- planned OCR text-embedding index but no code ever wrote to or read from it.
-- CREATE TABLE IF NOT EXISTS on existing DBs leaves the empty table behind,
-- so drop it explicitly (safe on fresh DBs too).
DROP TABLE IF EXISTS ocr_text_embeddings;
