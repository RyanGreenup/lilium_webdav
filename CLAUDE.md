# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Run Commands

```bash
# Build
cargo build
cargo build --release

# Run server
cargo run -- serve --database <path.db> -u <username> -P <password>

# Run with custom host/port
cargo run -- serve -d notes.db -u admin -P secret -H 0.0.0.0 -p 8080

# Test WebDAV with curl
curl -u username:password http://127.0.0.1:4918/ -X PROPFIND --header "Depth: 1"
curl -u username:password http://127.0.0.1:4918/path/to/file.md
```

## Architecture

This is a read-write WebDAV server that exposes a SQLite database (folders/notes) as a virtual filesystem.

### Core Flow

1. **CLI** (`src/cli.rs`) - Clap-based CLI with `serve` subcommand
2. **Commands** (`src/commands.rs`) - Sets up hyper HTTP server, handles Basic Auth, delegates to dav-server
3. **WebDAV** (`src/webdav/`) - Implements dav-server traits:
   - `filesystem.rs` - `DavFileSystem` trait: path resolution, directory listing, metadata
   - `davfile.rs` - `DavFile` trait: file reading (notes content)
   - `auth.rs` - Basic Auth header parsing

### Path Resolution

Virtual filesystem maps database structure to paths:
- `/` → root folders/notes where `parent_id IS NULL`
- `/FolderName/` → folder lookup by title + parent_id
- `/Folder/Note.md` → note lookup by title + syntax + parent_id

The `resolve_path()` method in `filesystem.rs` walks path components, URL-decodes them, and queries SQLite to find the corresponding folder or note.

### Database Schema

Uses `folders` and `notes` tables with hierarchical parent_id references. Notes have a `syntax` field (e.g., "md") that becomes the file extension.
