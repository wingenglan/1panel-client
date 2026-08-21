PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS server_groups (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS servers (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  host TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22 CHECK (port BETWEEN 1 AND 65535),
  username TEXT NOT NULL,
  auth_type TEXT NOT NULL CHECK (auth_type IN ('password', 'private_key', 'ssh_agent')),
  password_secret_ref TEXT,
  private_key_path TEXT,
  private_key_secret_ref TEXT,
  key_passphrase_secret_ref TEXT,
  sudo_mode TEXT NOT NULL DEFAULT 'none' CHECK (sudo_mode IN ('none', 'passwordless', 'password')),
  sudo_secret_ref TEXT,
  group_id TEXT REFERENCES server_groups(id) ON DELETE SET NULL,
  favorite INTEGER NOT NULL DEFAULT 0,
  settings_json TEXT NOT NULL DEFAULT '{}',
  last_connected_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_servers_group ON servers(group_id);
CREATE INDEX IF NOT EXISTS idx_servers_last_connected ON servers(last_connected_at DESC);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS server_tags (
  server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (server_id, tag_id)
);

CREATE TABLE IF NOT EXISTS known_hosts (
  server_identity TEXT PRIMARY KEY NOT NULL,
  key_type TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  public_key TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS recent_paths (
  server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  visited_at TEXT NOT NULL,
  PRIMARY KEY (server_id, path)
);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY NOT NULL,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY NOT NULL,
  server_id TEXT REFERENCES servers(id) ON DELETE SET NULL,
  action TEXT NOT NULL,
  resource_type TEXT NOT NULL,
  resource_id TEXT,
  result TEXT NOT NULL,
  summary TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_server_created ON audit_events(server_id, created_at DESC);
