-- Play Integrity verdicts of the last 30 days, by app version and outcome.
-- Read-only. Record-only signal: no verdict ever changed what the app or the
-- Worker did. Run with:
--   npx wrangler d1 execute telemetry --remote --file sql/integrity-report.sql
SELECT substr(received_at, 1, 10) AS day,
       source,
       app_version,
       status,
       reason,
       app_recognition,
       device_labels,
       licensing,
       play_protect,
       app_access_risk,
       COUNT(*) AS n
FROM integrity_verdicts
GROUP BY day, source, app_version, status, reason, app_recognition, device_labels, licensing, play_protect,
         app_access_risk
ORDER BY day DESC, n DESC;
