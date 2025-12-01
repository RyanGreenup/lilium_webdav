# WebDAV Server for SQLite Notes

A WebDAV server that exposes folders and notes from a SQLite database as a virtual filesystem. Supports reading existing notes and creating/updating notes via WebDAV clients.

## Building

```bash
cargo build --release
```

## Usage

```bash
webdav_server serve --database <DATABASE> --username <USERNAME> --password <PASSWORD> [OPTIONS]
```

Then on the client something like this

```

doas umount -l /home/ryan/Downloads/testing_webdav; doas mount -t davfs -o username=ryan http://localhost:4918 ~/Downloads/testing_webdav

```


or

1. Add yourself to the group
    ```sh
    get ent group davfs2 || sudo groupadd davfs2
    usermod -aG davfs2 $USER
    ```

2. Create credentials
    ```sh
    mkdir -p ~/.davfs2
    touch ~/.davfs2/secrets
    chmod 600 ~/.davfs2/secrets
    nvim ~/.davfs2/secrets

    ```

    ```
    http://localhost:4918 ryan 1234
    ```

3. Specify the mount in `fstab` (I believe the `noauto` option is mostly respected)

    ```
    http://localhost:4918 /home/ryan/Downloads/testing_webdav/ davfs ryan,noauto 0 0
    ```

    On systemd run `doas systemctl daemon-reload`

4. Logout and back int. If you don't feel like login/logout, start a new shell with `newgrp davfs2`. Although this didn't work for me YMMV

5. One of:

    ```sh
    mount ~/Downloads/testing_webdav

    ```

    ```sh
    mount http://<server>:<port> -o user -o noauto -t davfs ~/Downloads/testing_webdav

    ```
















### Required Options

| Option | Description |
|--------|-------------|
| `-d, --database <DATABASE>` | Path to the SQLite database |
| `-u, --username <USERNAME>` | Login username for Basic Auth |
| `-P, --password <PASSWORD>` | Password for Basic Auth |

### Optional Options

| Option | Default | Description |
|--------|---------|-------------|
| `-H, --host <HOST>` | `127.0.0.1` | Host address to bind to |
| `-p, --port <PORT>` | `4918` | Port to listen on |
| `--user-id <USER_ID>` | (username) | User ID in the database to filter content |

## Authentication

The server uses HTTP Basic Authentication. You must specify the credentials when starting the server:

```bash
# Simple setup - username matches database user_id
./webdav_server serve -d notes.db -u myuser -P mypassword

# Map login username to a different database user_id
./webdav_server serve -d notes.db -u admin -P secret --user-id abc123def456
```

When connecting via a WebDAV client or browser:
- **Username**: The value specified with `-u, --username`
- **Password**: The value specified with `-P, --password`

The `--user-id` option allows you to map the login username to a different user_id in the database. This is useful when:
- Your database uses UUIDs or other non-human-readable IDs
- You want a friendly login name that differs from the database identifier

If `--user-id` is not specified, the username is used directly to filter database content.

## Examples

```bash
# Start server on default port (4918)
./webdav_server serve -d /path/to/notes.db -u admin -P secret123

# Start server on custom host and port
./webdav_server serve -d notes.db -u admin -P secret123 -H 0.0.0.0 -p 8080

# Use a different user_id for database queries
./webdav_server serve -d notes.db -u webdav -P mypass --user-id user_550e8400-e29b
```

## Connecting

### Browser
Navigate to `http://localhost:4918/` and enter your credentials when prompted.

### macOS Finder
1. Go to **Finder > Go > Connect to Server** (⌘K)
2. Enter: `http://localhost:4918/`
3. Enter your username and password

### Windows Explorer
1. Right-click **This PC > Map network drive**
2. Enter: `http://localhost:4918/`
3. Check "Connect using different credentials"
4. Enter your username and password

### Linux (GNOME Files)
1. Press Ctrl+L to show the address bar
2. Enter: `dav://localhost:4918/`
3. Enter your username and password

## File Operations

The server supports both reading and writing files via WebDAV:

### Reading Files
- Browse folders and read existing notes from the database
- Download files using any WebDAV client

### Creating/Updating Files
- Create new notes by uploading files (e.g., `PUT /newfile.md`)
- Update existing notes by overwriting files
- File extension determines the note's `syntax` field (e.g., `.md` → `syntax='md'`)
- Notes are created in the folder corresponding to the path hierarchy

**Examples using curl:**
```bash
# Create a new note at the root
curl -u username:password -T newfile.md http://localhost:4918/newfile.md

# Update an existing note
echo "Updated content" | curl -u username:password -T - http://localhost:4918/existing.md

# Create a note in a folder
curl -u username:password -T note.md http://localhost:4918/FolderName/note.md
```

**Note:** Folder creation is not supported. Files can only be created within existing folders in the database.

## Database Schema

The server expects tables matching this schema:

```sql
CREATE TABLE folders (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  parent_id TEXT,
  user_id TEXT NOT NULL,
  created_at DATETIME,
  updated_at DATETIME
);

CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content TEXT NOT NULL,
  syntax TEXT NOT NULL DEFAULT 'md',
  parent_id TEXT,
  user_id TEXT NOT NULL,
  created_at DATETIME,
  updated_at DATETIME
);
```

- **folders**: Hierarchical directories (parent_id references another folder)
- **notes**: Files with content (parent_id references a folder, syntax determines file extension)
