-- Telemetry schema 5: Play Integrity verdicts, record-only.
--
-- The Android app's Play builds attach a Standard Play Integrity token to
-- their once-a-day automatic report batch and to a bug report they send. The
-- Worker asks Google to decode it and keeps one row per token: Google's coarse
-- labels and the SDK level, never the token, the app's certificate digest or
-- any identifier. Rows carry no install id; a bug report's row names its
-- upload. No verdict changes how anything is stored, answered or unlocked.
--
-- status is ok (decoded, and the package, request hash and token age
-- matched), mismatch (one of those failed; reason says which, and no verdict
-- is kept) or unevaluated (no service-account secret, malformed reply or a
-- Google error; reason says which). A NULL verdict column means Google didn't
-- evaluate it. device_labels and app_access_risk are sorted, comma-joined
-- labels; "" means Google returned none.
--
-- Deleted after RAW_RETENTION_DAYS (30) by the daily cron. Apply this before
-- deploying the Worker that writes it. It is additive, and that Worker treats
-- a failed insert as a lost verdict, never a failed request.

CREATE TABLE integrity_verdicts (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  received_at      TEXT    NOT NULL,     -- server time, ISO 8601 UTC
  source           TEXT    NOT NULL,     -- 'events' or 'bug'
  upload_id        INTEGER,              -- diag_uploads.id for a bug report
  app_version      TEXT    NOT NULL,
  status           TEXT    NOT NULL,
  reason           TEXT,
  app_recognition  TEXT,                 -- appIntegrity.appRecognitionVerdict
  app_version_code INTEGER,              -- appIntegrity.versionCode
  device_labels    TEXT,                 -- deviceIntegrity.deviceRecognitionVerdict
  device_activity  TEXT,                 -- recentDeviceActivity.deviceActivityLevel
  device_sdk       INTEGER,              -- deviceAttributes.sdkVersion
  licensing        TEXT,                 -- accountDetails.appLicensingVerdict
  play_protect     TEXT,                 -- environmentDetails.playProtectVerdict
  app_access_risk  TEXT                  -- appAccessRiskVerdict.appsDetected
);
CREATE INDEX integrity_verdicts_received_at ON integrity_verdicts (received_at);
CREATE INDEX integrity_verdicts_upload ON integrity_verdicts (upload_id) WHERE upload_id IS NOT NULL;
