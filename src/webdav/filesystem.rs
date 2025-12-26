use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::SystemTime;

use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, DavProp, FsError, FsFuture, FsResult,
    FsStream, OpenOptions, ReadDirMeta,
};
use futures_util::stream;
use percent_encoding::percent_decode_str;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::davfile::SqliteDavFile;

/// A WebDAV filesystem backed by SQLite
#[derive(Clone)]
pub struct SqliteFs {
    db_path: PathBuf,
    user_id: String,
}

impl SqliteFs {
    pub fn new(db_path: PathBuf, user_id: String) -> Self {
        Self { db_path, user_id }
    }

    fn open_db(&self) -> FsResult<Connection> {
        let conn = Connection::open(&self.db_path).map_err(|_| FsError::GeneralFailure)?;

        // Enable foreign keys - SQLite has them disabled by default.
        // This is required for ON DELETE CASCADE to work when deleting folders.
        conn.execute("PRAGMA foreign_keys = ON", [])
            .map_err(|_| FsError::GeneralFailure)?;

        Ok(conn)
    }

    /// Resolve a path to (parent_folder_id, entry_name, is_file)
    /// Returns (None, name, _) for root-level items
    fn resolve_path(&self, path: &DavPath) -> FsResult<ResolvedPath> {
        let path_str = path.as_url_string();
        let path_str = path_str.trim_start_matches('/').trim_end_matches('/');

        if path_str.is_empty() {
            return Ok(ResolvedPath::Root);
        }

        // URL-decode each path component
        let components: Vec<String> = path_str
            .split('/')
            .map(|c| percent_decode_str(c).decode_utf8_lossy().to_string())
            .collect();
        let conn = self.open_db()?;

        // Walk the path to find the parent folder
        let mut current_folder_id: Option<String> = None;

        for (i, component) in components.iter().enumerate() {
            let is_last = i == components.len() - 1;

            if is_last {
                // Check if it's a file (note) or folder
                if let Some((title, syntax)) = parse_filename(component) {
                    // Try to find as note
                    let note_exists =
                        self.note_exists(&conn, current_folder_id.as_deref(), &title, &syntax)?;
                    if note_exists {
                        return Ok(ResolvedPath::Note {
                            parent_id: current_folder_id,
                            title,
                            syntax,
                        });
                    }
                }

                // Try to find as folder
                if let Some(folder_id) =
                    self.find_folder(&conn, current_folder_id.as_deref(), component)?
                {
                    return Ok(ResolvedPath::Folder { id: folder_id });
                }

                return Err(FsError::NotFound);
            } else {
                // Not the last component - must be a folder
                current_folder_id =
                    self.find_folder(&conn, current_folder_id.as_deref(), component)?;
                if current_folder_id.is_none() {
                    return Err(FsError::NotFound);
                }
            }
        }

        Err(FsError::NotFound)
    }

    fn find_folder(
        &self,
        conn: &Connection,
        parent_id: Option<&str>,
        title: &str,
    ) -> FsResult<Option<String>> {
        let result = if let Some(pid) = parent_id {
            conn.query_row(
                "SELECT id FROM folders WHERE title = ? AND parent_id = ? AND user_id = ?",
                params![title, pid, self.user_id],
                |row| row.get::<_, String>(0),
            )
        } else {
            conn.query_row(
                "SELECT id FROM folders WHERE title = ? AND parent_id IS NULL AND user_id = ?",
                params![title, self.user_id],
                |row| row.get::<_, String>(0),
            )
        };

        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(_) => Err(FsError::GeneralFailure),
        }
    }

    fn note_exists(
        &self,
        conn: &Connection,
        parent_id: Option<&str>,
        title: &str,
        syntax: &str,
    ) -> FsResult<bool> {
        let result = if let Some(pid) = parent_id {
            conn.query_row(
                "SELECT 1 FROM notes WHERE title = ? AND syntax = ? AND parent_id = ? AND user_id = ?",
                params![title, syntax, pid, self.user_id],
                |_| Ok(()),
            )
        } else {
            conn.query_row(
                "SELECT 1 FROM notes WHERE title = ? AND syntax = ? AND parent_id IS NULL AND user_id = ?",
                params![title, syntax, self.user_id],
                |_| Ok(()),
            )
        };

        match result {
            Ok(()) => Ok(true),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(_) => Err(FsError::GeneralFailure),
        }
    }

    fn get_note(&self, parent_id: Option<&str>, title: &str, syntax: &str) -> FsResult<NoteData> {
        let conn = self.open_db()?;

        let result = if let Some(pid) = parent_id {
            conn.query_row(
                "SELECT id, title, content, syntax, created_at, updated_at FROM notes WHERE title = ? AND syntax = ? AND parent_id = ? AND user_id = ?",
                params![title, syntax, pid, self.user_id],
                |row| Ok(NoteData {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    syntax: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                }),
            )
        } else {
            conn.query_row(
                "SELECT id, title, content, syntax, created_at, updated_at FROM notes WHERE title = ? AND syntax = ? AND parent_id IS NULL AND user_id = ?",
                params![title, syntax, self.user_id],
                |row| Ok(NoteData {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    syntax: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                }),
            )
        };

        result.map_err(|_| FsError::NotFound)
    }

    fn get_folder_meta(&self, folder_id: &str) -> FsResult<FolderData> {
        let conn = self.open_db()?;

        conn.query_row(
            "SELECT id, title, created_at, updated_at FROM folders WHERE id = ? AND user_id = ?",
            params![folder_id, self.user_id],
            |row| {
                Ok(FolderData {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .map_err(|_| FsError::NotFound)
    }

    fn list_entries(&self, parent_id: Option<&str>) -> FsResult<Vec<DirEntry>> {
        let conn = self.open_db()?;
        let mut entries = Vec::new();

        // List folders using a helper to avoid closure type issues
        self.collect_folders(&conn, parent_id, &mut entries)?;
        self.collect_notes(&conn, parent_id, &mut entries)?;

        Ok(entries)
    }

    fn collect_folders(
        &self,
        conn: &Connection,
        parent_id: Option<&str>,
        entries: &mut Vec<DirEntry>,
    ) -> FsResult<()> {
        let folder_query = match parent_id {
            Some(pid) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, created_at, updated_at FROM folders WHERE parent_id = ? AND user_id = ?"
                ).map_err(|_| FsError::GeneralFailure)?;
                let rows = stmt
                    .query_map(params![pid, self.user_id], |row| {
                        Ok(FolderData {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            created_at: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    })
                    .map_err(|_| FsError::GeneralFailure)?;
                rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, created_at, updated_at FROM folders WHERE parent_id IS NULL AND user_id = ?"
                ).map_err(|_| FsError::GeneralFailure)?;
                let rows = stmt
                    .query_map(params![self.user_id], |row| {
                        Ok(FolderData {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            created_at: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    })
                    .map_err(|_| FsError::GeneralFailure)?;
                rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
            }
        };

        for f in folder_query {
            entries.push(DirEntry::Folder(f));
        }
        Ok(())
    }

    fn collect_notes(
        &self,
        conn: &Connection,
        parent_id: Option<&str>,
        entries: &mut Vec<DirEntry>,
    ) -> FsResult<()> {
        let note_query = match parent_id {
            Some(pid) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, content, syntax, created_at, updated_at FROM notes WHERE parent_id = ? AND user_id = ?"
                ).map_err(|_| FsError::GeneralFailure)?;
                let rows = stmt
                    .query_map(params![pid, self.user_id], |row| {
                        Ok(NoteData {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            content: row.get(2)?,
                            syntax: row.get(3)?,
                            created_at: row.get(4)?,
                            updated_at: row.get(5)?,
                        })
                    })
                    .map_err(|_| FsError::GeneralFailure)?;
                rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, content, syntax, created_at, updated_at FROM notes WHERE parent_id IS NULL AND user_id = ?"
                ).map_err(|_| FsError::GeneralFailure)?;
                let rows = stmt
                    .query_map(params![self.user_id], |row| {
                        Ok(NoteData {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            content: row.get(2)?,
                            syntax: row.get(3)?,
                            created_at: row.get(4)?,
                            updated_at: row.get(5)?,
                        })
                    })
                    .map_err(|_| FsError::GeneralFailure)?;
                rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
            }
        };

        for n in note_query {
            entries.push(DirEntry::Note(n));
        }
        Ok(())
    }

    /// Create or update a note in the database
    pub fn create_or_update_note(
        &self,
        parent_id: Option<&str>,
        title: &str,
        syntax: &str,
        content: &str,
    ) -> FsResult<String> {
        let conn = self.open_db()?;
        let timestamp = current_timestamp();

        // Check if note already exists
        let existing_id = if let Some(pid) = parent_id {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id = ? AND user_id = ?",
                params![title, syntax, pid, self.user_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
        } else {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id IS NULL AND user_id = ?",
                params![title, syntax, self.user_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
        };

        if let Some(id) = existing_id {
            // Update existing note
            conn.execute(
                "UPDATE notes SET content = ?, updated_at = ? WHERE id = ?",
                params![content, timestamp, id],
            )
            .map_err(|_| FsError::GeneralFailure)?;
            Ok(id)
        } else {
            // Create new note
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO notes (id, title, content, syntax, parent_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![id, title, content, syntax, parent_id, self.user_id, timestamp, timestamp],
            )
            .map_err(|_| FsError::GeneralFailure)?;
            Ok(id)
        }
    }

    /// Create a new folder in the database
    pub fn create_folder(&self, parent_id: Option<&str>, title: &str) -> FsResult<String> {
        let conn = self.open_db()?;
        let timestamp = current_timestamp();

        // Check if folder already exists
        let existing_folder = self.find_folder(&conn, parent_id, title)?;

        if let Some(_id) = existing_folder {
            // Folder already exists, return error
            eprintln!("[CREATE_FOLDER] Folder '{}' already exists", title);
            return Err(FsError::Exists);
        }

        // Create new folder
        let id = Uuid::new_v4().to_string();
        eprintln!("[CREATE_FOLDER] Creating folder '{}' with id: {}", title, id);

        conn.execute(
            "INSERT INTO folders (id, title, parent_id, user_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
            params![id, title, parent_id, self.user_id, timestamp, timestamp],
        )
        .map_err(|e| {
            eprintln!("[CREATE_FOLDER] Database error: {}", e);
            FsError::GeneralFailure
        })?;

        Ok(id)
    }

    /// Delete a note from the database
    pub fn delete_note(&self, parent_id: Option<&str>, title: &str, syntax: &str) -> FsResult<()> {
        let conn = self.open_db()?;

        // Find the note ID first
        let note_id: String = if let Some(pid) = parent_id {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id = ? AND user_id = ?",
                params![title, syntax, pid, self.user_id],
                |row| row.get(0),
            )
        } else {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id IS NULL AND user_id = ?",
                params![title, syntax, self.user_id],
                |row| row.get(0),
            )
        }
        .map_err(|e| {
            eprintln!("[DELETE_NOTE] Note not found: {}", e);
            FsError::NotFound
        })?;

        eprintln!("[DELETE_NOTE] Deleting note '{}' with id: {}", title, note_id);

        conn.execute(
            "DELETE FROM notes WHERE id = ? AND user_id = ?",
            params![note_id, self.user_id],
        )
        .map_err(|e| {
            eprintln!("[DELETE_NOTE] Database error: {}", e);
            FsError::GeneralFailure
        })?;

        Ok(())
    }

    /// Delete a folder from the database.
    ///
    /// This relies on CASCADE DELETE defined in the schema:
    /// - Child folders are deleted via: FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE
    /// - Child notes are deleted via: FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE
    ///
    /// IMPORTANT: Foreign keys must be enabled (PRAGMA foreign_keys = ON) for CASCADE to work.
    /// This is done in open_db().
    pub fn delete_folder(&self, folder_id: &str) -> FsResult<()> {
        let conn = self.open_db()?;

        eprintln!("[DELETE_FOLDER] Deleting folder with id: {}", folder_id);

        // Verify the folder exists and belongs to this user
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM folders WHERE id = ? AND user_id = ?",
                params![folder_id, self.user_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !exists {
            eprintln!("[DELETE_FOLDER] Folder not found or not owned by user");
            return Err(FsError::NotFound);
        }

        // Delete the folder - CASCADE will handle children
        let deleted = conn
            .execute(
                "DELETE FROM folders WHERE id = ? AND user_id = ?",
                params![folder_id, self.user_id],
            )
            .map_err(|e| {
                eprintln!("[DELETE_FOLDER] Database error: {}", e);
                FsError::GeneralFailure
            })?;

        if deleted == 0 {
            eprintln!("[DELETE_FOLDER] No rows deleted");
            return Err(FsError::NotFound);
        }

        eprintln!("[DELETE_FOLDER] Folder deleted successfully (CASCADE handled children)");
        Ok(())
    }

    /// Rename/move a note in the database
    pub fn rename_note(
        &self,
        src_parent_id: Option<&str>,
        src_title: &str,
        src_syntax: &str,
        dst_parent_id: Option<&str>,
        dst_title: &str,
        dst_syntax: &str,
    ) -> FsResult<()> {
        let conn = self.open_db()?;
        let timestamp = current_timestamp();

        // Find the source note
        let note_id: String = if let Some(pid) = src_parent_id {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id = ? AND user_id = ?",
                params![src_title, src_syntax, pid, self.user_id],
                |row| row.get(0),
            )
        } else {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id IS NULL AND user_id = ?",
                params![src_title, src_syntax, self.user_id],
                |row| row.get(0),
            )
        }
        .map_err(|e| {
            eprintln!("[RENAME_NOTE] Source note not found: {}", e);
            FsError::NotFound
        })?;

        eprintln!(
            "[RENAME_NOTE] Found source note id={}, renaming '{}.{}' -> '{}.{}'",
            note_id, src_title, src_syntax, dst_title, dst_syntax
        );

        // Check if destination already exists and delete it (MOVE overwrites)
        let existing_dst_id: Option<String> = if let Some(pid) = dst_parent_id {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id = ? AND user_id = ?",
                params![dst_title, dst_syntax, pid, self.user_id],
                |row| row.get(0),
            )
            .ok()
        } else {
            conn.query_row(
                "SELECT id FROM notes WHERE title = ? AND syntax = ? AND parent_id IS NULL AND user_id = ?",
                params![dst_title, dst_syntax, self.user_id],
                |row| row.get(0),
            )
            .ok()
        };

        if let Some(dst_id) = existing_dst_id {
            if dst_id != note_id {
                eprintln!("[RENAME_NOTE] Overwriting existing note at destination: {}", dst_id);
                conn.execute(
                    "DELETE FROM notes WHERE id = ? AND user_id = ?",
                    params![dst_id, self.user_id],
                )
                .map_err(|e| {
                    eprintln!("[RENAME_NOTE] Failed to delete destination: {}", e);
                    FsError::GeneralFailure
                })?;
            }
        }

        // Update the note with new title, syntax, and parent_id
        conn.execute(
            "UPDATE notes SET title = ?, syntax = ?, parent_id = ?, updated_at = ? WHERE id = ? AND user_id = ?",
            params![dst_title, dst_syntax, dst_parent_id, timestamp, note_id, self.user_id],
        )
        .map_err(|e| {
            eprintln!("[RENAME_NOTE] Database error: {}", e);
            FsError::GeneralFailure
        })?;

        eprintln!("[RENAME_NOTE] Note renamed successfully");
        Ok(())
    }

    /// Rename/move a folder in the database
    pub fn rename_folder(
        &self,
        folder_id: &str,
        new_parent_id: Option<&str>,
        new_title: &str,
    ) -> FsResult<()> {
        let conn = self.open_db()?;
        let timestamp = current_timestamp();

        // Verify the source folder exists and belongs to this user
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM folders WHERE id = ? AND user_id = ?",
                params![folder_id, self.user_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !exists {
            eprintln!("[RENAME_FOLDER] Source folder not found: {}", folder_id);
            return Err(FsError::NotFound);
        }

        // Check if destination folder name already exists in target parent
        let existing_dst = self.find_folder(&conn, new_parent_id, new_title)?;

        if let Some(dst_id) = existing_dst {
            if dst_id != folder_id {
                // Destination folder already exists and is different from source
                // Unlike files, we don't overwrite folders - return error
                eprintln!(
                    "[RENAME_FOLDER] Destination folder '{}' already exists",
                    new_title
                );
                return Err(FsError::Exists);
            }
            // If dst_id == folder_id, it's a no-op (renaming to same name)
        }

        // Prevent moving a folder into itself or its descendants
        if let Some(new_pid) = new_parent_id {
            if new_pid == folder_id {
                eprintln!("[RENAME_FOLDER] Cannot move folder into itself");
                return Err(FsError::Forbidden);
            }
            // Check if new_parent_id is a descendant of folder_id
            if self.is_descendant(&conn, new_pid, folder_id)? {
                eprintln!("[RENAME_FOLDER] Cannot move folder into its own descendant");
                return Err(FsError::Forbidden);
            }
        }

        eprintln!(
            "[RENAME_FOLDER] Renaming folder {} to '{}' with parent {:?}",
            folder_id, new_title, new_parent_id
        );

        // Update the folder
        conn.execute(
            "UPDATE folders SET title = ?, parent_id = ?, updated_at = ? WHERE id = ? AND user_id = ?",
            params![new_title, new_parent_id, timestamp, folder_id, self.user_id],
        )
        .map_err(|e| {
            eprintln!("[RENAME_FOLDER] Database error: {}", e);
            FsError::GeneralFailure
        })?;

        eprintln!("[RENAME_FOLDER] Folder renamed successfully");
        Ok(())
    }

    /// Check if potential_descendant is a descendant of ancestor_id
    fn is_descendant(
        &self,
        conn: &Connection,
        potential_descendant: &str,
        ancestor_id: &str,
    ) -> FsResult<bool> {
        let mut current_id = Some(potential_descendant.to_string());

        while let Some(id) = current_id {
            if id == ancestor_id {
                return Ok(true);
            }
            // Get parent of current folder
            current_id = conn
                .query_row(
                    "SELECT parent_id FROM folders WHERE id = ? AND user_id = ?",
                    params![id, self.user_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten();
        }

        Ok(false)
    }
}

/// Get current timestamp in SQLite format
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap();

    let secs = duration.as_secs();
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let remaining = remaining % 3600;
    let minutes = remaining / 60;
    let seconds = remaining % 60;

    // Approximate date calculation (not accounting for leap years properly)
    let year = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = 1 + (day_of_year / 30);
    let day = 1 + (day_of_year % 30);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

enum ResolvedPath {
    Root,
    Folder {
        id: String,
    },
    Note {
        parent_id: Option<String>,
        title: String,
        syntax: String,
    },
}

#[derive(Clone, Debug)]
pub struct NoteData {
    #[allow(unused)]
    pub id: String,
    pub title: String,
    pub content: String,
    pub syntax: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
struct FolderData {
    #[allow(unused)]
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
}

enum DirEntry {
    Folder(FolderData),
    Note(NoteData),
}

/// Parse a filename like "note.md" into ("note", "md")
fn parse_filename(name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = name.rsplitn(2, '.').collect();
    if parts.len() == 2 {
        Some((parts[1].to_string(), parts[0].to_string()))
    } else {
        None
    }
}

/// Metadata for files/folders
#[derive(Clone, Debug)]
struct SqliteMetaData {
    is_dir: bool,
    len: u64,
    modified: SystemTime,
    created: SystemTime,
}

impl DavMetaData for SqliteMetaData {
    fn len(&self) -> u64 {
        self.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.modified)
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn created(&self) -> FsResult<SystemTime> {
        Ok(self.created)
    }
}

/// Directory entry for listings
struct SqliteDirEntry {
    name: String,
    metadata: SqliteMetaData,
}

impl DavDirEntry for SqliteDirEntry {
    fn name(&self) -> Vec<u8> {
        self.name.clone().into_bytes()
    }

    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = self.metadata.clone();
        Box::pin(async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) })
    }
}

impl DavFileSystem for SqliteFs {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            eprintln!("[OPEN] path={}, create={}, write={}, append={}, create_new={}",
                path.as_url_string(), options.create, options.write, options.append, options.create_new);

            // Try to resolve the existing path first
            match fs.resolve_path(&path) {
                Ok(ResolvedPath::Note {
                    parent_id,
                    title,
                    syntax,
                }) => {
                    // File exists
                    eprintln!("[OPEN] File exists: title={}, syntax={}", title, syntax);
                    let note = fs.get_note(parent_id.as_deref(), &title, &syntax)?;

                    // Return writable or read-only based on options
                    if options.write || options.append {
                        eprintln!("[OPEN] Returning writable file (truncate={})", options.truncate);
                        Ok(Box::new(SqliteDavFile::new_writable(note, fs, parent_id, options.truncate)) as Box<dyn DavFile>)
                    } else {
                        eprintln!("[OPEN] Returning read-only file");
                        Ok(Box::new(SqliteDavFile::new(note, fs)) as Box<dyn DavFile>)
                    }
                }
                Ok(_) => {
                    eprintln!("[OPEN] Path resolved to folder or root - forbidden");
                    Err(FsError::Forbidden)
                }
                Err(FsError::NotFound) if options.create || options.create_new => {
                    eprintln!("[OPEN] File not found, creating new file");
                    // File doesn't exist, but we're allowed to create it
                    let path_str = path.as_url_string();
                    let path_str = path_str.trim_start_matches('/').trim_end_matches('/');

                    if path_str.is_empty() {
                        eprintln!("[OPEN] Empty path - forbidden");
                        return Err(FsError::Forbidden);
                    }

                    // URL-decode path components
                    let components: Vec<String> = path_str
                        .split('/')
                        .map(|c| percent_decode_str(c).decode_utf8_lossy().to_string())
                        .collect();

                    // The last component is the filename
                    let filename = components.last().ok_or(FsError::Forbidden)?;
                    eprintln!("[OPEN] Filename: {}", filename);

                    // Parse filename to get title and syntax
                    let (title, syntax) = parse_filename(filename).ok_or_else(|| {
                        eprintln!("[OPEN] Failed to parse filename - no extension?");
                        FsError::Forbidden
                    })?;
                    eprintln!("[OPEN] Parsed: title={}, syntax={}", title, syntax);

                    // Walk the path to find the parent folder (if any)
                    let conn = fs.open_db()?;
                    let mut parent_id: Option<String> = None;

                    for component in components.iter().take(components.len() - 1) {
                        parent_id = fs.find_folder(&conn, parent_id.as_deref(), component)?;
                        if parent_id.is_none() {
                            eprintln!("[OPEN] Parent folder '{}' not found", component);
                            return Err(FsError::NotFound);
                        }
                    }
                    eprintln!("[OPEN] Parent folder resolved: {:?}", parent_id);

                    // Create an empty note initially
                    eprintln!("[OPEN] Creating note in database");
                    let note_id = fs.create_or_update_note(
                        parent_id.as_deref(),
                        &title,
                        &syntax,
                        "",
                    )?;
                    eprintln!("[OPEN] Note created with id: {}", note_id);

                    // Return a writable file (always truncate for new files)
                    let note = NoteData {
                        id: note_id,
                        title,
                        content: String::new(),
                        syntax,
                        created_at: current_timestamp(),
                        updated_at: current_timestamp(),
                    };

                    Ok(Box::new(SqliteDavFile::new_writable(note, fs, parent_id, true)) as Box<dyn DavFile>)
                }
                Err(e) => {
                    eprintln!("[OPEN] Error resolving path: {:?}", e);
                    Err(e)
                }
            }
        })
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            let parent_id = match fs.resolve_path(&path)? {
                ResolvedPath::Root => None,
                ResolvedPath::Folder { id } => Some(id),
                ResolvedPath::Note { .. } => return Err(FsError::Forbidden),
            };

            let entries = fs.list_entries(parent_id.as_deref())?;
            let dir_entries: Vec<Box<dyn DavDirEntry>> = entries
                .into_iter()
                .map(|e| {
                    let (name, metadata) = match e {
                        DirEntry::Folder(f) => {
                            let meta = SqliteMetaData {
                                is_dir: true,
                                len: 0,
                                modified: parse_datetime(&f.updated_at),
                                created: parse_datetime(&f.created_at),
                            };
                            (f.title, meta)
                        }
                        DirEntry::Note(n) => {
                            let filename = format!("{}.{}", n.title, n.syntax);
                            let meta = SqliteMetaData {
                                is_dir: false,
                                len: n.content.len() as u64,
                                modified: parse_datetime(&n.updated_at),
                                created: parse_datetime(&n.created_at),
                            };
                            (filename, meta)
                        }
                    };
                    Box::new(SqliteDirEntry { name, metadata }) as Box<dyn DavDirEntry>
                })
                .collect();

            Ok(Box::pin(stream::iter(dir_entries.into_iter().map(Ok)))
                as FsStream<Box<dyn DavDirEntry>>)
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            match fs.resolve_path(&path)? {
                ResolvedPath::Root => {
                    let meta = SqliteMetaData {
                        is_dir: true,
                        len: 0,
                        modified: SystemTime::now(),
                        created: SystemTime::now(),
                    };
                    Ok(Box::new(meta) as Box<dyn DavMetaData>)
                }
                ResolvedPath::Folder { id } => {
                    let folder = fs.get_folder_meta(&id)?;
                    let meta = SqliteMetaData {
                        is_dir: true,
                        len: 0,
                        modified: parse_datetime(&folder.updated_at),
                        created: parse_datetime(&folder.created_at),
                    };
                    Ok(Box::new(meta) as Box<dyn DavMetaData>)
                }
                ResolvedPath::Note {
                    parent_id,
                    title,
                    syntax,
                } => {
                    let note = fs.get_note(parent_id.as_deref(), &title, &syntax)?;
                    let meta = SqliteMetaData {
                        is_dir: false,
                        len: note.content.len() as u64,
                        modified: parse_datetime(&note.updated_at),
                        created: parse_datetime(&note.created_at),
                    };
                    Ok(Box::new(meta) as Box<dyn DavMetaData>)
                }
            }
        })
    }

    fn symlink_metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        // No symlinks in our filesystem, just return regular metadata
        self.metadata(path)
    }

    fn have_props<'a>(
        &'a self,
        _path: &'a DavPath,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            // Signal that we support WebDAV properties
            // This tells clients we're DAV-compliant
            true
        })
    }

    fn get_props<'a>(
        &'a self,
        path: &'a DavPath,
        _do_content: bool,
    ) -> FsFuture<'a, Vec<DavProp>> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            eprintln!("[GET_PROPS] path={}", path.as_url_string());

            // Return empty properties for now
            // This is enough to signal DAV compliance
            match fs.resolve_path(&path) {
                Ok(_) => Ok(vec![]),
                Err(FsError::NotFound) => Ok(vec![]), // Even for non-existent files
                Err(e) => Err(e),
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            eprintln!("[CREATE_DIR] path={}", path.as_url_string());

            // Parse the path
            let path_str = path.as_url_string();
            let path_str = path_str.trim_start_matches('/').trim_end_matches('/');

            if path_str.is_empty() {
                eprintln!("[CREATE_DIR] Cannot create root directory");
                return Err(FsError::Forbidden);
            }

            // URL-decode path components
            let components: Vec<String> = path_str
                .split('/')
                .map(|c| percent_decode_str(c).decode_utf8_lossy().to_string())
                .collect();

            // The last component is the folder name
            let folder_name = components.last().ok_or(FsError::Forbidden)?;
            eprintln!("[CREATE_DIR] Folder name: {}", folder_name);

            // Walk the path to find the parent folder (if any)
            let conn = fs.open_db()?;
            let mut parent_id: Option<String> = None;

            for component in components.iter().take(components.len() - 1) {
                parent_id = fs.find_folder(&conn, parent_id.as_deref(), component)?;
                if parent_id.is_none() {
                    eprintln!("[CREATE_DIR] Parent folder '{}' not found", component);
                    return Err(FsError::NotFound);
                }
            }
            eprintln!("[CREATE_DIR] Parent folder resolved: {:?}", parent_id);

            // Create the folder
            fs.create_folder(parent_id.as_deref(), folder_name)?;
            eprintln!("[CREATE_DIR] Folder '{}' created successfully", folder_name);

            Ok(())
        })
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            eprintln!("[REMOVE_FILE] path={}", path.as_url_string());

            // Resolve the path to get the note details
            match fs.resolve_path(&path)? {
                ResolvedPath::Note {
                    parent_id,
                    title,
                    syntax,
                } => {
                    eprintln!("[REMOVE_FILE] Deleting note: title={}, syntax={}", title, syntax);
                    fs.delete_note(parent_id.as_deref(), &title, &syntax)?;
                    eprintln!("[REMOVE_FILE] Note '{}' deleted successfully", title);
                    Ok(())
                }
                ResolvedPath::Folder { .. } => {
                    eprintln!("[REMOVE_FILE] Cannot delete folder with remove_file");
                    Err(FsError::Forbidden)
                }
                ResolvedPath::Root => {
                    eprintln!("[REMOVE_FILE] Cannot delete root");
                    Err(FsError::Forbidden)
                }
            }
        })
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        let path = path.clone();
        let fs = self.clone();

        Box::pin(async move {
            eprintln!("[REMOVE_DIR] path={}", path.as_url_string());

            // Resolve the path to get the folder ID
            match fs.resolve_path(&path)? {
                ResolvedPath::Folder { id } => {
                    eprintln!("[REMOVE_DIR] Deleting folder with id: {}", id);
                    // delete_folder relies on CASCADE to remove all children
                    // (child folders and notes). See schema.sql for FK definitions.
                    fs.delete_folder(&id)?;
                    eprintln!("[REMOVE_DIR] Folder deleted successfully");
                    Ok(())
                }
                ResolvedPath::Note { .. } => {
                    eprintln!("[REMOVE_DIR] Cannot delete file with remove_dir");
                    Err(FsError::Forbidden)
                }
                ResolvedPath::Root => {
                    eprintln!("[REMOVE_DIR] Cannot delete root directory");
                    Err(FsError::Forbidden)
                }
            }
        })
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        let from = from.clone();
        let to = to.clone();
        let fs = self.clone();

        Box::pin(async move {
            eprintln!("[RENAME] from={} to={}", from.as_url_string(), to.as_url_string());

            // Parse the destination path first (common to both notes and folders)
            let to_str = to.as_url_string();
            let to_str = to_str.trim_start_matches('/').trim_end_matches('/');

            if to_str.is_empty() {
                eprintln!("[RENAME] Destination path is empty");
                return Err(FsError::Forbidden);
            }

            // URL-decode path components
            let components: Vec<String> = to_str
                .split('/')
                .map(|c| percent_decode_str(c).decode_utf8_lossy().to_string())
                .collect();

            let dst_name = components.last().ok_or(FsError::Forbidden)?.clone();

            // Resolve destination parent folder
            let conn = fs.open_db()?;
            let mut dst_parent_id: Option<String> = None;

            for component in components.iter().take(components.len() - 1) {
                dst_parent_id = fs.find_folder(&conn, dst_parent_id.as_deref(), component)?;
                if dst_parent_id.is_none() {
                    eprintln!("[RENAME] Destination parent folder '{}' not found", component);
                    return Err(FsError::NotFound);
                }
            }

            // Resolve the source path and handle based on type
            match fs.resolve_path(&from)? {
                ResolvedPath::Note {
                    parent_id: src_parent_id,
                    title: src_title,
                    syntax: src_syntax,
                } => {
                    eprintln!(
                        "[RENAME] Source note: parent_id={:?}, title={}, syntax={}",
                        src_parent_id, src_title, src_syntax
                    );

                    // Parse the destination filename for notes (requires extension)
                    let (dst_title, dst_syntax) = parse_filename(&dst_name).ok_or_else(|| {
                        eprintln!("[RENAME] Invalid destination filename (no extension): {}", dst_name);
                        FsError::Forbidden
                    })?;

                    eprintln!(
                        "[RENAME] Destination note: parent_id={:?}, title={}, syntax={}",
                        dst_parent_id, dst_title, dst_syntax
                    );

                    // Perform the note rename
                    fs.rename_note(
                        src_parent_id.as_deref(),
                        &src_title,
                        &src_syntax,
                        dst_parent_id.as_deref(),
                        &dst_title,
                        &dst_syntax,
                    )?;
                }
                ResolvedPath::Folder { id: folder_id } => {
                    eprintln!("[RENAME] Source folder: id={}", folder_id);
                    eprintln!(
                        "[RENAME] Destination folder: parent_id={:?}, title={}",
                        dst_parent_id, dst_name
                    );

                    // Perform the folder rename
                    fs.rename_folder(&folder_id, dst_parent_id.as_deref(), &dst_name)?;
                }
                ResolvedPath::Root => {
                    eprintln!("[RENAME] Cannot rename root");
                    return Err(FsError::Forbidden);
                }
            }

            eprintln!("[RENAME] Rename completed successfully");
            Ok(())
        })
    }
}

/// Parse SQLite datetime string to SystemTime
fn parse_datetime(s: &str) -> SystemTime {
    // SQLite stores as "YYYY-MM-DD HH:MM:SS"
    // For simplicity, return current time on parse failure
    use std::time::Duration;

    // Simple parsing - in production use chrono
    let parts: Vec<&str> = s.split(&['-', ' ', ':'][..]).collect();
    if parts.len() >= 6 {
        if let (Ok(year), Ok(month), Ok(day), Ok(hour), Ok(min), Ok(sec)) = (
            parts[0].parse::<i64>(),
            parts[1].parse::<i64>(),
            parts[2].parse::<i64>(),
            parts[3].parse::<i64>(),
            parts[4].parse::<i64>(),
            parts[5].parse::<i64>(),
        ) {
            // Approximate: days since epoch
            let days_since_epoch = (year - 1970) * 365 + (month - 1) * 30 + day;
            let secs = (days_since_epoch * 86400 + hour * 3600 + min * 60 + sec) as u64;
            return SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        }
    }
    SystemTime::now()
}
