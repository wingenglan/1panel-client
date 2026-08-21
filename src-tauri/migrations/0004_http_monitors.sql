PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS http_monitors (
  id TEXT PRIMARY KEY NOT NULL,
  server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  expected_status INTEGER,
  interval_seconds INTEGER NOT NULL DEFAULT 300 CHECK (interval_seconds BETWEEN 30 AND 86400),
  enabled INTEGER NOT NULL DEFAULT 1,
  last_checked_at TEXT,
  last_reachable INTEGER,
  last_status_code INTEGER,
  last_latency_ms INTEGER,
  last_detail TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_http_monitors_server_enabled
  ON http_monitors(server_id, enabled, updated_at DESC);

CREATE TABLE IF NOT EXISTS http_monitor_checks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  monitor_id TEXT NOT NULL REFERENCES http_monitors(id) ON DELETE CASCADE,
  checked_at TEXT NOT NULL,
  reachable INTEGER NOT NULL,
  status_code INTEGER,
  latency_ms INTEGER,
  detail TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_http_monitor_checks_monitor_time
  ON http_monitor_checks(monitor_id, checked_at DESC);
