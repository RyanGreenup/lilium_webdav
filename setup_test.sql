CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  title TEXT NOT NULL,
  parent_id TEXT,
  user_id TEXT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS notes (
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
    UNIQUE(parent_id, title, syntax)
  );

-- Insert test data
INSERT INTO folders (id, title, user_id) VALUES ('folder1', 'Documents', 'testuser');
INSERT INTO folders (id, title, parent_id, user_id) VALUES ('folder2', 'Work', 'folder1', 'testuser');
INSERT INTO notes (id, title, content, syntax, user_id) VALUES ('note1', 'Welcome', '# Welcome\n\nThis is a test note at the root level.', 'md', 'testuser');
INSERT INTO notes (id, title, content, syntax, parent_id, user_id) VALUES ('note2', 'Project Ideas', '# Project Ideas\n\n- WebDAV server\n- Todo app\n- Blog engine', 'md', 'folder1', 'testuser');
INSERT INTO notes (id, title, content, syntax, parent_id, user_id) VALUES ('note3', 'Meeting Notes', '# Meeting Notes\n\nDiscussed the roadmap for Q1.', 'md', 'folder2', 'testuser');
