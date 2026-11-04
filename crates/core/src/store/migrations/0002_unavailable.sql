-- 0002: reason a provider has no countable quota (set by the collector, cleared by a normal run).

ALTER TABLE source_status ADD COLUMN unavailable TEXT;
