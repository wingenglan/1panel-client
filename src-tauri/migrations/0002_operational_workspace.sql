PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS command_shortcuts (
  id TEXT PRIMARY KEY NOT NULL,
  scope TEXT NOT NULL CHECK (scope IN ('global', 'server')),
  server_id TEXT REFERENCES servers(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  command_template TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  tags_json TEXT NOT NULL DEFAULT '[]',
  enabled INTEGER NOT NULL DEFAULT 1,
  builtin INTEGER NOT NULL DEFAULT 0,
  usage_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CHECK ((scope = 'global' AND server_id IS NULL) OR (scope = 'server' AND server_id IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_shortcuts_scope_name
  ON command_shortcuts(scope, COALESCE(server_id, ''), name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_shortcuts_server_enabled
  ON command_shortcuts(server_id, enabled, usage_count DESC);

CREATE TABLE IF NOT EXISTS metric_samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
  sampled_at TEXT NOT NULL,
  cpu_usage_percent REAL,
  memory_used_bytes INTEGER NOT NULL,
  memory_total_bytes INTEGER NOT NULL,
  load_one REAL NOT NULL,
  network_rx_bytes_per_second INTEGER NOT NULL,
  network_tx_bytes_per_second INTEGER NOT NULL,
  disk_usage_percent REAL
);

CREATE INDEX IF NOT EXISTS idx_metric_samples_server_time
  ON metric_samples(server_id, sampled_at DESC);

CREATE TABLE IF NOT EXISTS task_records (
  id TEXT PRIMARY KEY NOT NULL,
  type TEXT NOT NULL,
  server_id TEXT REFERENCES servers(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'success', 'failed', 'cancelled', 'interrupted')),
  progress REAL,
  bytes_transferred INTEGER NOT NULL DEFAULT 0,
  total_bytes INTEGER,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  error_code TEXT,
  error_message TEXT,
  cancel_supported INTEGER NOT NULL DEFAULT 1,
  retry_payload_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_records_updated
  ON task_records(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_task_records_server_status
  ON task_records(server_id, status, updated_at DESC);
