-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com

-- Replayed migration batches must not manufacture duplicate coverage spans.
DELETE FROM brain_history_coverage
WHERE id NOT IN (
    SELECT MIN(id)
    FROM brain_history_coverage
    GROUP BY start_at, end_at
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_brain_history_coverage_span
    ON brain_history_coverage(start_at, end_at);
