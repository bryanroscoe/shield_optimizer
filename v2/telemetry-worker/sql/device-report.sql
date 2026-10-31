-- Supported-device report straight from D1, all time, one row per TV model.
--
--   npx wrangler d1 execute telemetry --remote --file sql/device-report.sql
--
-- Reads only the aggregate tables, so it still covers devices whose raw events
-- were purged after 90 days. An empty manufacturer/model is an error that was
-- not tied to a device.
WITH totals AS (
  SELECT manufacturer, model, device, profile,
         SUM(events) AS events, SUM(seen) AS seen,
         SUM(action_ok) AS action_ok, SUM(action_fail) AS action_fail,
         SUM(errors) AS errors, MAX(last_seen) AS last_seen
  FROM agg_device_daily
  GROUP BY manufacturer, model, device, profile
),
installs AS (
  SELECT manufacturer, model, device, profile, COUNT(DISTINCT install_id) AS installs
  FROM agg_device_installs
  GROUP BY manufacturer, model, device, profile
),
errs AS (
  SELECT manufacturer, model, device, profile, error_code, SUM(count) AS n,
         ROW_NUMBER() OVER (
           PARTITION BY manufacturer, model, device, profile ORDER BY SUM(count) DESC
         ) AS rk
  FROM agg_error_daily
  GROUP BY manufacturer, model, device, profile, error_code
)
SELECT t.manufacturer,
       t.model,
       t.device,
       t.profile,
       COALESCE(i.installs, 0) AS installs,
       t.seen,
       t.action_ok,
       t.action_fail,
       ROUND(100.0 * t.action_ok / NULLIF(t.action_ok + t.action_fail, 0), 1) AS ok_pct,
       t.errors,
       (SELECT group_concat(e.error_code || ' (' || e.n || ')', ', ')
          FROM errs e
         WHERE e.manufacturer = t.manufacturer AND e.model = t.model
           AND e.device = t.device AND e.profile = t.profile AND e.rk <= 3) AS top_errors,
       t.last_seen
FROM totals t
LEFT JOIN installs i USING (manufacturer, model, device, profile)
ORDER BY installs DESC, t.events DESC;
