use crate::error::{MigrationError, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Validates that the input database exists and is accessible
pub fn validate_input_database(path: &Path) -> Result<()> {
    // Check path exists
    if !path.exists() {
        return Err(MigrationError::InputNotFound(path.to_path_buf()));
    }

    // Check it's a file (not directory)
    if !path.is_file() {
        return Err(MigrationError::InputNotAFile(path.to_path_buf()));
    }

    // Try to open as SQLite database (read-only) to confirm it's valid
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| MigrationError::InvalidDatabase(path.to_path_buf(), e.to_string()))?;

    Ok(())
}

/// Validates that the output path is writable
pub fn validate_output_path(path: &Path) -> Result<()> {
    // If path exists, check it's a file (not directory)
    if path.exists() && !path.is_file() {
        return Err(MigrationError::OutputNotAFile(path.to_path_buf()));
    }

    // Check parent directory exists
    if let Some(parent) = path.parent() {
        // Handle empty parent (current directory) - always valid
        if parent.as_os_str().is_empty() || parent == Path::new(".") {
            // Parent is current directory, which always exists in running context
            return Ok(());
        }

        // For non-empty parents, check if they exist
        if !parent.exists() {
            return Err(MigrationError::ParentDirNotFound(parent.to_path_buf()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_validate_output_path_relative_current_dir() {
        // Should succeed - parent is current directory
        let path = PathBuf::from("new.sqlite");
        assert!(validate_output_path(&path).is_ok());
    }

    #[test]
    fn test_validate_output_path_explicit_current_dir() {
        // Should succeed - parent is explicit "."
        let path = PathBuf::from("./new.sqlite");
        assert!(validate_output_path(&path).is_ok());
    }

    #[test]
    fn test_validate_output_path_nonexistent_subdir() {
        // Should fail - subdirectory doesn't exist
        let path = PathBuf::from("definitely_nonexistent_directory_12345/new.sqlite");
        assert!(validate_output_path(&path).is_err());
    }

    #[test]
    fn test_validate_output_path_absolute_nonexistent() {
        // Should fail - absolute path with nonexistent parent
        let path = PathBuf::from("/definitely_nonexistent_path_xyz_12345/new.sqlite");
        let result = validate_output_path(&path);
        assert!(result.is_err());
        if let Err(MigrationError::ParentDirNotFound(_)) = result {
            // Expected error type
        } else {
            panic!("Expected ParentDirNotFound error");
        }
    }
}
