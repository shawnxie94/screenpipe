-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
-- Keep journal sequence numbers one-to-one with durable deletion waves.
-- Historical crash rows may still use the old zero sentinel, so the index
-- intentionally covers only allocated positive sequences.
CREATE UNIQUE INDEX IF NOT EXISTS idx_brain_deletions_journal_seq
    ON brain_deletions(journal_seq)
    WHERE journal_seq > 0;
