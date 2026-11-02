-- Telemetry schema 3: connection diagnostics, log excerpts and Send logs.
--
-- Client event schema 2 adds the `diagnostic` kind (a scan or device read
-- that found nothing usable) and two optional fields on any event:
--   diag         structured counts, codes and versions, rebuilt key by key by
--                the Worker (src/diag.ts) and stored as JSON text. No free text.
--   log_excerpt  desktop only: the redacted tail of the app's adb and
--                connection log lines, at most 60 lines / 8 KiB, redacted
--                again by the Worker. Cleared after LOG_RETENTION_DAYS (30).
--
-- diag_uploads holds what a user sent with "Send logs to developer": a
-- redacted summary plus up to 256 KiB of the app log, keyed by the install id
-- the app shows as its support ID. Deleted after LOG_RETENTION_DAYS (30).
--
-- Nothing here holds a request IP or any location.
--
-- Apply this BEFORE deploying the Worker that writes these columns: that
-- Worker inserts `diag` and `log_excerpt` into every event row, so without the
-- columns every ingest would answer 503. It is additive, so the previous
-- Worker keeps working against it in the meantime.

ALTER TABLE events ADD COLUMN diag TEXT;
ALTER TABLE events ADD COLUMN log_excerpt TEXT;

-- The support lookup finds an install's events by full id or 8-hex prefix.
CREATE INDEX events_install ON events (install_id, received_at);

CREATE TABLE diag_uploads (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  received_at TEXT    NOT NULL,           -- server time, ISO 8601 UTC
  client_ts   TEXT    NOT NULL,
  install_id  TEXT    NOT NULL,
  app         TEXT    NOT NULL,
  app_version TEXT    NOT NULL,
  os          TEXT    NOT NULL,
  reports_on  INTEGER,                    -- automatic reports on when sent
  bytes       INTEGER NOT NULL,
  body        TEXT    NOT NULL            -- redacted text
);
CREATE INDEX diag_uploads_install ON diag_uploads (install_id, received_at);
CREATE INDEX diag_uploads_received_at ON diag_uploads (received_at);
