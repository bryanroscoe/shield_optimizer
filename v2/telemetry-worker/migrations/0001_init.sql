-- Telemetry schema 1.
--
-- Raw rows live in `events` and are purged after RAW_RETENTION_DAYS (90) by the
-- cron trigger. The agg_* tables are what the device report reads and are kept.
-- No column anywhere holds a request IP, a TV serial, a TV name, a package list,
-- a file path or licence data; the Worker never receives the ones it could avoid
-- and drops the rest during validation.

CREATE TABLE events (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  received_at     TEXT    NOT NULL,           -- server time, ISO 8601 UTC
  client_ts       TEXT    NOT NULL,           -- client clock, normalised to ISO 8601 UTC
  install_id      TEXT    NOT NULL,
  app             TEXT    NOT NULL,
  app_version     TEXT    NOT NULL,
  os              TEXT    NOT NULL,
  country         TEXT,                       -- request.cf.country only, nothing finer
  kind            TEXT    NOT NULL,
  manufacturer    TEXT,
  brand           TEXT,
  model           TEXT,
  device          TEXT,
  product         TEXT,
  android_release TEXT,
  sdk             INTEGER,
  profile         TEXT,
  support_tier    TEXT,
  action          TEXT,
  ok              INTEGER,
  error_code      TEXT,
  message         TEXT
);
CREATE INDEX events_received_at ON events (received_at);

-- Dimension columns use '' when the event had no device, so they can be part of
-- the primary key.
CREATE TABLE agg_device_daily (
  day          TEXT    NOT NULL,
  manufacturer TEXT    NOT NULL,
  model        TEXT    NOT NULL,
  device       TEXT    NOT NULL,
  profile      TEXT    NOT NULL,
  app          TEXT    NOT NULL,
  app_version  TEXT    NOT NULL,
  events       INTEGER NOT NULL DEFAULT 0,
  seen         INTEGER NOT NULL DEFAULT 0,
  action_ok    INTEGER NOT NULL DEFAULT 0,
  action_fail  INTEGER NOT NULL DEFAULT 0,
  errors       INTEGER NOT NULL DEFAULT 0,
  last_seen    TEXT    NOT NULL,
  PRIMARY KEY (day, manufacturer, model, device, profile, app, app_version)
) WITHOUT ROWID;

-- One row per random install id per device group, so the report can count
-- distinct installs at any grouping without double counting upgrades.
CREATE TABLE agg_device_installs (
  manufacturer TEXT NOT NULL,
  model        TEXT NOT NULL,
  device       TEXT NOT NULL,
  profile      TEXT NOT NULL,
  app          TEXT NOT NULL,
  app_version  TEXT NOT NULL,
  install_id   TEXT NOT NULL,
  first_seen   TEXT NOT NULL,
  last_seen    TEXT NOT NULL,
  PRIMARY KEY (manufacturer, model, device, profile, app, app_version, install_id)
) WITHOUT ROWID;

CREATE TABLE agg_error_daily (
  day          TEXT    NOT NULL,
  manufacturer TEXT    NOT NULL,
  model        TEXT    NOT NULL,
  device       TEXT    NOT NULL,
  profile      TEXT    NOT NULL,
  app          TEXT    NOT NULL,
  app_version  TEXT    NOT NULL,
  error_code   TEXT    NOT NULL,
  count        INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (day, manufacturer, model, device, profile, app, app_version, error_code)
) WITHOUT ROWID;

-- Abuse caps. Purged by the cron trigger after a couple of days.
CREATE TABLE rate_install_daily (
  day        TEXT    NOT NULL,
  install_id TEXT    NOT NULL,
  events     INTEGER NOT NULL,
  PRIMARY KEY (day, install_id)
) WITHOUT ROWID;

CREATE TABLE rate_global_daily (
  day    TEXT    PRIMARY KEY,
  events INTEGER NOT NULL
) WITHOUT ROWID;

-- Served by GET /v1/support-tiers, replaced by PUT /v1/admin/support-tiers.
-- First matching rule (lowest position) wins. Informational only.
CREATE TABLE support_tier_rules (
  position     INTEGER PRIMARY KEY,
  manufacturer TEXT,
  brand        TEXT,
  model_prefix TEXT,
  device       TEXT,
  tier         TEXT NOT NULL CHECK (tier IN ('unknown', 'not_supported', 'beta', 'supported', 'fully_supported')),
  note         TEXT NOT NULL
);

CREATE TABLE support_tier_meta (
  id      INTEGER PRIMARY KEY CHECK (id = 1),
  updated TEXT    NOT NULL
);

-- Seed. SHIELD strings come from the scrubbed getprop capture in
-- v2/crates/core/tests/fixtures/devices/shield-tv-pro. The repo has no TCL
-- getprop capture; "TCL" is the manufacturer/brand reported for the TCL QM7L Pro
-- (Android 14) in issue #88, so the rule matches on manufacturer alone.
INSERT INTO support_tier_rules (position, manufacturer, brand, model_prefix, device, tier, note) VALUES
  (1, 'NVIDIA', NULL, 'SHIELD', NULL, 'fully_supported',
   'NVIDIA SHIELD TV is the primary test device.'),
  (2, 'TCL', NULL, NULL, NULL, 'beta',
   'TCL Google TV is in beta: tested less than SHIELD. Please report anything that does not work.'),
  (3, 'Amazon', NULL, 'AFT', NULL, 'not_supported',
   'Fire OS blocks the changes this app needs to make, so Fire TV is not supported.');

INSERT INTO support_tier_meta (id, updated) VALUES (1, '2026-10-31T00:00:00Z');
