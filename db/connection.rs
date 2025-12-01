use crate::error::Result;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Holds connections to both input and output databases
pub struct DbConnections {
    pub input: Connection,
    pub output: Connection,
}

impl DbConnections {
    /// Creates new database connections for input (read-only) and output (read-write)
    pub fn new(input_path: &Path, output_path: &Path) -> Result<Self> {
        let input = Self::open_read_only(input_path)?;
        let output = Self::open_write(output_path)?;

        Ok(DbConnections { input, output })
    }

    /// Opens a database connection in read-only mode
    fn open_read_only(path: &Path) -> Result<Connection> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(conn)
    }

    /// Opens a database connection in read-write mode, creating if it doesn't exist
    fn open_write(path: &Path) -> Result<Connection> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        Ok(conn)
    }
}
