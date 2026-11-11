-- Read-only: the last 14 days' event counts and their stored size as
-- GLOBAL_DAILY_EVENT_BYTES counts it (string bytes plus 256 per row, the same
-- columns as EVENT_SIZE_SQL in src/ingest.ts). Used to size that cap; see
-- README "Production".
WITH sized AS (
  SELECT
    substr(received_at, 1, 10) AS day,
    256 + LENGTH(CAST(
      COALESCE(install_id, '') || COALESCE(app, '') || COALESCE(app_version, '') || COALESCE(os, '') ||
      COALESCE(client_ts, '') || COALESCE(kind, '') || COALESCE(manufacturer, '') || COALESCE(brand, '') ||
      COALESCE(model, '') || COALESCE(device, '') || COALESCE(product, '') || COALESCE(android_release, '') ||
      COALESCE(profile, '') || COALESCE(support_tier, '') || COALESCE(action, '') || COALESCE(error_code, '') ||
      COALESCE(message, '') || COALESCE(diag, '') || COALESCE(log_excerpt, '')
    AS BLOB)) AS bytes
  FROM events
  WHERE received_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-14 days')
)
SELECT day, COUNT(*) AS events, CAST(AVG(bytes) AS INTEGER) AS avg_bytes, SUM(bytes) AS day_bytes
FROM sized
GROUP BY day
ORDER BY day;
