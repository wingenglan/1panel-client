ALTER TABLE command_shortcuts ADD COLUMN group_name TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_shortcuts_group
  ON command_shortcuts(group_name COLLATE NOCASE, scope, name COLLATE NOCASE);
