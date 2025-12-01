use std::fmt::Debug;
use std::io::{Cursor, SeekFrom};
use std::time::SystemTime;

use bytes::{Buf, Bytes};
use dav_server::fs::{DavFile, DavMetaData, FsError, FsFuture, FsResult};

use super::filesystem::{NoteData, SqliteFs};

/// A WebDAV file backed by a note from SQLite
pub struct SqliteDavFile {
    note: NoteData,
    cursor: Cursor<Vec<u8>>,
    writable: bool,
    fs: Option<SqliteFs>,
    parent_id: Option<String>,
}

impl Debug for SqliteDavFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteDavFile")
            .field("note", &self.note)
            .field("writable", &self.writable)
            .finish()
    }
}

impl SqliteDavFile {
    pub fn new(note: NoteData, fs: SqliteFs) -> Self {
        let content = note.content.clone().into_bytes();
        Self {
            note,
            cursor: Cursor::new(content),
            writable: false,
            fs: Some(fs),
            parent_id: None,
        }
    }

    pub fn new_writable(note: NoteData, fs: SqliteFs, parent_id: Option<String>) -> Self {
        let content = note.content.clone().into_bytes();
        Self {
            note,
            cursor: Cursor::new(content),
            writable: true,
            fs: Some(fs),
            parent_id,
        }
    }
}

/// Metadata for note files
#[derive(Debug, Clone)]
struct NoteMetaData {
    len: u64,
    modified: SystemTime,
    created: SystemTime,
}

impl DavMetaData for NoteMetaData {
    fn len(&self) -> u64 {
        self.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.modified)
    }

    fn is_dir(&self) -> bool {
        false
    }

    fn created(&self) -> FsResult<SystemTime> {
        Ok(self.created)
    }
}

impl DavFile for SqliteDavFile {
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let len = self.note.content.len() as u64;
        let modified = parse_datetime(&self.note.updated_at);
        let created = parse_datetime(&self.note.created_at);

        Box::pin(async move {
            Ok(Box::new(NoteMetaData {
                len,
                modified,
                created,
            }) as Box<dyn DavMetaData>)
        })
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        use std::io::Read;

        let mut buf = vec![0u8; count];
        let n = self.cursor.read(&mut buf).unwrap_or(0);
        buf.truncate(n);

        Box::pin(async move { Ok(Bytes::from(buf)) })
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        use std::io::Seek;

        let result = self.cursor.seek(pos).map_err(|_| FsError::GeneralFailure);
        Box::pin(async move { result })
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()> {
        if !self.writable {
            return Box::pin(async { Err(FsError::Forbidden) });
        }

        use std::io::Write;

        // Write to the cursor
        let result = self.cursor.write_all(&buf).map_err(|_| FsError::GeneralFailure);
        Box::pin(async move { result })
    }

    fn write_buf(&mut self, mut buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        if !self.writable {
            return Box::pin(async { Err(FsError::Forbidden) });
        }

        use std::io::Write;

        // Convert Buf to bytes and write
        let bytes = buf.copy_to_bytes(buf.remaining());
        let result = self.cursor.write_all(&bytes).map_err(|_| FsError::GeneralFailure);
        Box::pin(async move { result })
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        if !self.writable {
            return Box::pin(async { Ok(()) });
        }

        // Get the content from the cursor
        let content = match std::str::from_utf8(self.cursor.get_ref()) {
            Ok(s) => s.to_string(),
            Err(_) => return Box::pin(async { Err(FsError::GeneralFailure) }),
        };

        // Update the note in the database
        if let Some(ref fs) = self.fs {
            let fs = fs.clone();
            let parent_id = self.parent_id.clone();
            let title = self.note.title.clone();
            let syntax = self.note.syntax.clone();

            Box::pin(async move {
                fs.create_or_update_note(
                    parent_id.as_deref(),
                    &title,
                    &syntax,
                    &content,
                ).map(|_| ())
            })
        } else {
            Box::pin(async { Err(FsError::GeneralFailure) })
        }
    }
}

/// Parse SQLite datetime string to SystemTime
fn parse_datetime(s: &str) -> SystemTime {
    use std::time::Duration;

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
            let days_since_epoch = (year - 1970) * 365 + (month - 1) * 30 + day;
            let secs = (days_since_epoch * 86400 + hour * 3600 + min * 60 + sec) as u64;
            return SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        }
    }
    SystemTime::now()
}
