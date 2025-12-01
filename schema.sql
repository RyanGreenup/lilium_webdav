
CREATE TABLE folders (
  id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  title TEXT NOT NULL,
  parent_id TEXT,
  user_id TEXT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE
);
CREATE INDEX idx_folders_user_id ON folders(user_id);
CREATE INDEX idx_folders_parent_id ON folders(parent_id);
CREATE INDEX idx_folders_parent_user_title ON folders(parent_id, user_id, title);
CREATE INDEX idx_folders_root_user_title ON folders(user_id, title) WHERE parent_id IS NULL;
CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    abstract TEXT,
    content TEXT NOT NULL,
    syntax TEXT NOT NULL DEFAULT 'md',
    parent_id TEXT,
    user_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE,
    -- Include syntax to for pandoc sake
    UNIQUE(parent_id, title, syntax)
  );
CREATE INDEX idx_notes_user_id ON notes(user_id);
CREATE INDEX idx_notes_parent_id ON notes(parent_id);
CREATE INDEX idx_notes_syntax ON notes(syntax);
CREATE INDEX idx_notes_updated_at ON notes(updated_at);
CREATE INDEX idx_notes_parent_title ON notes(parent_id, title);
CREATE INDEX idx_notes_parent_updated ON notes(parent_id, updated_at);
CREATE INDEX idx_notes_content ON notes(content IS NULL);
CREATE INDEX idx_notes_parent_user ON notes(parent_id, user_id);
CREATE INDEX idx_notes_root_user_title ON notes(user_id, title) WHERE parent_id IS NULL;
CREATE VIRTUAL TABLE notes_fts USING fts5(
      id UNINDEXED,
      title,
      abstract,
      content,
      user_id UNINDEXED
    )
/* notes_fts(id,title,abstract,content,user_id) */;
CREATE TABLE IF NOT EXISTS 'notes_fts_data'(id INTEGER PRIMARY KEY, block BLOB);
CREATE TABLE IF NOT EXISTS 'notes_fts_idx'(segid, term, pgno, PRIMARY KEY(segid, term)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS 'notes_fts_content'(id INTEGER PRIMARY KEY, c0, c1, c2, c3, c4);
CREATE TABLE IF NOT EXISTS 'notes_fts_docsize'(id INTEGER PRIMARY KEY, sz BLOB);
CREATE TABLE IF NOT EXISTS 'notes_fts_config'(k PRIMARY KEY, v) WITHOUT ROWID;
CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
      INSERT INTO notes_fts(id, title, abstract, content, user_id)
      VALUES (new.id, new.title, new.abstract, new.content, new.user_id);
    END;
CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
    DELETE FROM notes_fts WHERE id = old.id;
END;
CREATE TRIGGER notes_fts_update AFTER UPDATE ON notes BEGIN
    DELETE FROM notes_fts WHERE id = old.id;
    INSERT INTO notes_fts(id, title, abstract, content, user_id)
    VALUES (new.id, new.title, new.abstract, new.content, new.user_id);
END;
CREATE TABLE notes_history (
      id TEXT,
      title TEXT NOT NULL,
      abstract TEXT,
      content TEXT NOT NULL,
      syntax TEXT NOT NULL DEFAULT 'md',
      -- Why it was logged
      log_action TEXT CHECK (log_action IN ('DELETE', 'UPDATE')) DEFAULT 'DELETE',
      parent_id TEXT,
      user_id TEXT NOT NULL,
      created_at DATETIME,
      updated_at DATETIME,
      deleted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
      history_id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16))))
  );
CREATE INDEX idx_notes_history_user_id ON notes_history(user_id);
CREATE INDEX idx_notes_history_parent_id ON notes_history(parent_id);
CREATE INDEX idx_notes_history_deleted_at ON notes_history(deleted_at);
CREATE INDEX idx_notes_history_id ON notes_history(id);
CREATE TRIGGER notes_before_update
BEFORE UPDATE ON notes
BEGIN
    -- Copy the old version of the note to notes_history
    INSERT INTO notes_history (id, title, abstract, content, syntax, log_action, parent_id, user_id, created_at, updated_at, deleted_at)
    VALUES (old.id, old.title, old.abstract, old.content, old.syntax, 'UPDATE', old.parent_id, old.user_id, old.created_at, old.updated_at, CURRENT_TIMESTAMP);

    -- Delete older history entries beyond the last 30 for this note where log_action is 'UPDATE'
    DELETE FROM notes_history
    WHERE id = old.id
    AND log_action = 'UPDATE'
    AND history_id NOT IN (
        SELECT history_id FROM notes_history
        WHERE id = old.id
        AND log_action = 'UPDATE'
        ORDER BY deleted_at DESC
        LIMIT 30
    );
END;
CREATE TRIGGER notes_before_delete
BEFORE DELETE ON notes
BEGIN
    -- Copy the note being deleted to notes_history
    INSERT INTO notes_history (id, title, abstract, content, syntax, log_action, parent_id, user_id, created_at, updated_at, deleted_at)
    VALUES (old.id, old.title, old.abstract, old.content, old.syntax, 'DELETE', old.parent_id, old.user_id, old.created_at, old.updated_at, CURRENT_TIMESTAMP);

    -- Delete older history entries beyond the last 15 for this note where log_action is 'DELETE'
    DELETE FROM notes_history
    WHERE id = old.id
    AND log_action = 'DELETE'
    AND history_id NOT IN (
        SELECT history_id FROM notes_history
        WHERE id = old.id
        AND log_action = 'DELETE'
        ORDER BY deleted_at DESC
        LIMIT 15
    );
END;
CREATE VIEW v_folder_id_path_mapping AS
WITH RECURSIVE folder_path AS (
    -- Base case: root folders (folders with no parent)
    SELECT
        id,
        title,
        parent_id,
        user_id,
        title AS path
    FROM folders
    WHERE parent_id IS NULL

    UNION ALL

    -- Recursive case: build path for nested folders
    SELECT
        f.id,
        f.title,
        f.parent_id,
        f.user_id,
        fp.path || '/' || f.title AS path
    FROM folders f
    INNER JOIN folder_path fp ON f.parent_id = fp.id
)
SELECT
    id,
    title,
    parent_id,
    user_id,
    path AS full_path
FROM folder_path
/* v_folder_id_path_mapping(id,title,parent_id,user_id,full_path) */;
CREATE VIEW v_note_id_path_mapping AS
WITH RECURSIVE folder_path AS (
    -- Base case: root folders (folders with no parent)
    SELECT
        id,
        title,
        parent_id,
        user_id,
        title AS path
    FROM folders
    WHERE parent_id IS NULL

    UNION ALL

    -- Recursive case: build path for nested folders
    SELECT
        f.id,
        f.title,
        f.parent_id,
        f.user_id,
        fp.path || '/' || f.title AS path
    FROM folders f
    INNER JOIN folder_path fp ON f.parent_id = fp.id
)
SELECT
    n.id,
    n.title,
    n.syntax,
    n.user_id,
    CASE
        WHEN n.parent_id IS NULL THEN n.title || '.' || n.syntax
        ELSE fp.path || '/' || n.title || '.' || n.syntax
    END AS full_path
FROM notes n
LEFT JOIN folder_path fp ON n.parent_id = fp.id
/* v_note_id_path_mapping(id,title,syntax,user_id,full_path) */;
