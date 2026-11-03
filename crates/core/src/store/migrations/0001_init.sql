-- 0001: snapshot and source_status.
-- Timestamps are UTC RFC 3339 text with nine fraction digits, so text order is time order.
-- No column holds credentials or tokens.

CREATE TABLE snapshot (
    provider    TEXT NOT NULL,
    "window"    TEXT NOT NULL,
    used        REAL NOT NULL CHECK (used >= 0),
    "limit"     REAL NOT NULL CHECK ("limit" >= 0),
    unit        TEXT NOT NULL CHECK (unit IN ('percent', 'requests')),
    resets_at   TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    source      TEXT NOT NULL CHECK (source IN ('poll', 'statusline')),
    UNIQUE (provider, "window", observed_at, source)
);

CREATE TABLE source_status (
    provider      TEXT PRIMARY KEY NOT NULL,
    last_ok_at    TEXT,
    last_error_at TEXT,
    last_error    TEXT
);
