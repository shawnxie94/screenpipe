-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com

-- Parsed sync orders by timestamp then id. Include the run id so SQLite can
-- check the handled run without visiting every unparsed frame's payload row.
CREATE INDEX IF NOT EXISTS idx_frames_parsed_timestamp
ON frames(timestamp, id, semantic_run_id)
WHERE semantic_run_id IS NOT NULL;
