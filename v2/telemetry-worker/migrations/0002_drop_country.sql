-- Telemetry schema 2: no location of any kind.
--
-- Owner decision 2026-11-01: the backend stops storing the Cloudflare-derived
-- country, so nothing it keeps is location, approximate or otherwise. The Worker
-- no longer reads request.cf at all. Clear the stored values first, then drop
-- the column. Only the raw `events` table ever had it; the aggregate tables never
-- grouped by country.
--
-- Apply this AFTER deploying the Worker that no longer writes `country`: the
-- previous Worker inserts into the column, so dropping it first would turn every
-- ingest into a 503 until the new code is live.

UPDATE events SET country = NULL WHERE country IS NOT NULL;
ALTER TABLE events DROP COLUMN country;
