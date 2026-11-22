-- Telemetry schema 4: bug reports.
--
-- POST /v1/diagnostics schema 2 adds `kind` ("logs" for Send logs, "bug" for
-- Report a bug) and `note`, the user's own description (at most 4000
-- characters, redacted by the Worker like the upload text). The phone app may
-- send schema 2 uploads; its text is capped at 64 KiB.
--
-- Both columns are covered by everything that already handles diag_uploads
-- rows: the 30-day purge, the storage-budget trim (`bytes` counts the note),
-- the support lookup and a per-install deletion.
--
-- Apply this BEFORE deploying the Worker that writes these columns: it inserts
-- `kind` and `note` into every upload row, so without them every upload would
-- answer 503. It is additive, so the previous Worker keeps working against it
-- in the meantime (its rows get kind 'logs' and no note).

ALTER TABLE diag_uploads ADD COLUMN kind TEXT NOT NULL DEFAULT 'logs';
ALTER TABLE diag_uploads ADD COLUMN note TEXT;
