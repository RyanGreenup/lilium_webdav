#!/usr/bin/env python3
"""
Comprehensive WebDAV CRUD tests for notes and folders.

Usage:
    1. Start the WebDAV server:
       ./target/release/webdav_server serve -d test.db -u testuser -P testpass

    2. Run tests:
       python tests/test_webdav.py

    Or with pytest:
       pytest tests/test_webdav.py -v
"""

import os
import subprocess
import tempfile
import time
import unittest
from urllib.parse import quote

import requests
from requests.auth import HTTPBasicAuth

# Configuration
BASE_URL = os.environ.get("WEBDAV_URL", "http://127.0.0.1:4918")
USERNAME = os.environ.get("WEBDAV_USER", "testuser")
PASSWORD = os.environ.get("WEBDAV_PASS", "testpass")

AUTH = HTTPBasicAuth(USERNAME, PASSWORD)


class WebDAVClient:
    """Simple WebDAV client for testing."""

    def __init__(self, base_url: str, auth: HTTPBasicAuth):
        self.base_url = base_url.rstrip("/")
        self.auth = auth
        self.session = requests.Session()
        self.session.auth = auth

    def _url(self, path: str) -> str:
        """Build full URL from path."""
        if not path.startswith("/"):
            path = "/" + path
        return self.base_url + path

    def get(self, path: str) -> requests.Response:
        """GET a resource (read file content)."""
        return self.session.get(self._url(path))

    def put(self, path: str, content: str) -> requests.Response:
        """PUT a resource (create/update file)."""
        return self.session.put(
            self._url(path),
            data=content.encode("utf-8"),
            headers={"Content-Type": "text/plain; charset=utf-8"},
        )

    def delete(self, path: str) -> requests.Response:
        """DELETE a resource (file or folder)."""
        return self.session.delete(self._url(path))

    def mkcol(self, path: str) -> requests.Response:
        """MKCOL - create a collection (folder)."""
        return self.session.request("MKCOL", self._url(path))

    def propfind(self, path: str, depth: int = 1) -> requests.Response:
        """PROPFIND - list directory contents."""
        return self.session.request(
            "PROPFIND",
            self._url(path),
            headers={"Depth": str(depth)},
        )

    def exists(self, path: str) -> bool:
        """Check if a resource exists."""
        resp = self.session.head(self._url(path))
        return resp.status_code == 200

    def list_dir(self, path: str) -> list[str]:
        """List directory contents, returning resource names."""
        resp = self.propfind(path, depth=1)
        if resp.status_code != 207:
            return []
        # Parse the multistatus XML response
        # Simple extraction of href values
        import re

        hrefs = re.findall(r"<D:href>([^<]+)</D:href>", resp.text)
        # Filter out the directory itself and extract names
        names = []
        base_path = path.rstrip("/") + "/"
        for href in hrefs:
            # Skip the directory itself
            if href.rstrip("/") == path.rstrip("/") or href == "/":
                continue
            # Extract the name from the href
            name = href.rstrip("/").split("/")[-1]
            if name:
                names.append(name)
        return names


class TestWebDAVAuth(unittest.TestCase):
    """Test authentication."""

    def test_unauthorized_without_auth(self):
        """Request without auth should return 401."""
        resp = requests.get(BASE_URL + "/")
        self.assertEqual(resp.status_code, 401)

    def test_unauthorized_wrong_password(self):
        """Request with wrong password should return 401."""
        resp = requests.get(BASE_URL + "/", auth=HTTPBasicAuth(USERNAME, "wrongpass"))
        self.assertEqual(resp.status_code, 401)

    def test_authorized_with_correct_credentials(self):
        """Request with correct credentials should succeed."""
        resp = requests.get(BASE_URL + "/", auth=AUTH)
        self.assertIn(resp.status_code, [200, 207])


class TestNoteCRUD(unittest.TestCase):
    """Test CRUD operations on notes (files)."""

    @classmethod
    def setUpClass(cls):
        cls.client = WebDAVClient(BASE_URL, AUTH)

    def test_01_create_note(self):
        """Create a new note via PUT."""
        content = "# Test Note\n\nThis is test content."
        resp = self.client.put("/test_create.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

    def test_02_read_note(self):
        """Read an existing note via GET."""
        # First create the note
        content = "# Read Test\n\nContent to read."
        self.client.put("/test_read.md", content)

        # Now read it back
        resp = self.client.get("/test_read.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

    def test_03_update_note(self):
        """Update an existing note via PUT."""
        # Create initial note
        initial_content = "# Initial Content"
        self.client.put("/test_update.md", initial_content)

        # Update the note
        updated_content = "# Updated Content\n\nThis has been modified."
        resp = self.client.put("/test_update.md", updated_content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Verify the update
        resp = self.client.get("/test_update.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, updated_content)

    def test_04_delete_note(self):
        """Delete a note via DELETE."""
        # Create a note to delete
        self.client.put("/test_delete.md", "To be deleted")

        # Delete it
        resp = self.client.delete("/test_delete.md")
        self.assertIn(resp.status_code, [200, 204])

        # Verify it's gone
        resp = self.client.get("/test_delete.md")
        self.assertEqual(resp.status_code, 404)

    def test_05_read_nonexistent_note(self):
        """Reading a non-existent note should return 404."""
        resp = self.client.get("/nonexistent_note_12345.md")
        self.assertEqual(resp.status_code, 404)

    def test_06_delete_nonexistent_note(self):
        """Deleting a non-existent note should return 404."""
        resp = self.client.delete("/nonexistent_note_12345.md")
        self.assertEqual(resp.status_code, 404)

    def test_07_create_note_with_spaces_in_name(self):
        """Create a note with spaces in the filename."""
        content = "# Spaced Note"
        path = "/My Test Note.md"
        resp = self.client.put(path, content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Read it back
        resp = self.client.get(path)
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

        # Clean up
        self.client.delete(path)

    def test_08_create_note_with_unicode(self):
        """Create a note with unicode content."""
        content = "# Unicode Test\n\n日本語テスト\némojis: 🎉🚀"
        resp = self.client.put("/unicode_test.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Read it back
        resp = self.client.get("/unicode_test.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

        # Clean up
        self.client.delete("/unicode_test.md")

    def test_09_create_note_different_extensions(self):
        """Create notes with different file extensions."""
        test_cases = [
            ("/test_file.txt", "Plain text content"),
            ("/test_file.json", '{"key": "value"}'),
            ("/test_file.html", "<html><body>Hello</body></html>"),
        ]

        for path, content in test_cases:
            with self.subTest(path=path):
                resp = self.client.put(path, content)
                self.assertIn(resp.status_code, [200, 201, 204])

                resp = self.client.get(path)
                self.assertEqual(resp.status_code, 200)
                self.assertEqual(resp.text, content)

                # Clean up
                self.client.delete(path)

    def test_10_large_note(self):
        """Create and read a large note."""
        # Create ~100KB of content
        content = "# Large Note\n\n" + ("x" * 1000 + "\n") * 100
        resp = self.client.put("/large_note.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Read it back
        resp = self.client.get("/large_note.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

        # Clean up
        self.client.delete("/large_note.md")


class TestFolderCRUD(unittest.TestCase):
    """Test CRUD operations on folders."""

    @classmethod
    def setUpClass(cls):
        cls.client = WebDAVClient(BASE_URL, AUTH)

    def test_01_create_folder(self):
        """Create a new folder via MKCOL."""
        resp = self.client.mkcol("/TestFolder")
        self.assertIn(resp.status_code, [200, 201])

    def test_02_list_folder(self):
        """List folder contents via PROPFIND."""
        # Create a folder
        self.client.mkcol("/ListTestFolder")

        # List root to see the folder
        resp = self.client.propfind("/")
        self.assertEqual(resp.status_code, 207)
        self.assertIn("ListTestFolder", resp.text)

    def test_03_create_note_in_folder(self):
        """Create a note inside a folder."""
        # Create folder first
        self.client.mkcol("/NoteTestFolder")

        # Create note in folder
        content = "# Note in Folder"
        resp = self.client.put("/NoteTestFolder/nested_note.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Read it back
        resp = self.client.get("/NoteTestFolder/nested_note.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

    def test_04_create_nested_folders(self):
        """Create nested folder structure."""
        # Create parent folder
        self.client.mkcol("/ParentFolder")

        # Create child folder
        resp = self.client.mkcol("/ParentFolder/ChildFolder")
        self.assertIn(resp.status_code, [200, 201])

        # Create note in nested folder
        content = "# Deeply Nested"
        resp = self.client.put("/ParentFolder/ChildFolder/deep_note.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Read it back
        resp = self.client.get("/ParentFolder/ChildFolder/deep_note.md")
        self.assertEqual(resp.status_code, 200)

    def test_05_delete_empty_folder(self):
        """Delete an empty folder."""
        # Create and then delete
        self.client.mkcol("/EmptyFolder")
        resp = self.client.delete("/EmptyFolder")
        self.assertIn(resp.status_code, [200, 204])

        # Verify it's gone
        resp = self.client.propfind("/EmptyFolder")
        self.assertEqual(resp.status_code, 404)

    def test_06_delete_folder_with_notes(self):
        """Delete a folder containing notes (recursive delete)."""
        # Create folder with notes
        self.client.mkcol("/FolderWithNotes")
        self.client.put("/FolderWithNotes/note1.md", "Note 1")
        self.client.put("/FolderWithNotes/note2.md", "Note 2")

        # Delete the folder
        resp = self.client.delete("/FolderWithNotes")
        self.assertIn(resp.status_code, [200, 204])

        # Verify folder is gone
        resp = self.client.propfind("/FolderWithNotes")
        self.assertEqual(resp.status_code, 404)

        # Verify notes are gone too
        resp = self.client.get("/FolderWithNotes/note1.md")
        self.assertEqual(resp.status_code, 404)

    def test_07_delete_folder_with_nested_folders(self):
        """Delete a folder containing nested folders (deep recursive delete)."""
        # Create nested structure
        self.client.mkcol("/DeepFolder")
        self.client.mkcol("/DeepFolder/Level1")
        self.client.mkcol("/DeepFolder/Level1/Level2")
        self.client.put("/DeepFolder/root_note.md", "Root note")
        self.client.put("/DeepFolder/Level1/l1_note.md", "Level 1 note")
        self.client.put("/DeepFolder/Level1/Level2/l2_note.md", "Level 2 note")

        # Delete the top-level folder
        resp = self.client.delete("/DeepFolder")
        self.assertIn(resp.status_code, [200, 204])

        # Verify everything is gone
        self.assertEqual(self.client.propfind("/DeepFolder").status_code, 404)
        self.assertEqual(self.client.get("/DeepFolder/root_note.md").status_code, 404)
        self.assertEqual(self.client.get("/DeepFolder/Level1/l1_note.md").status_code, 404)
        self.assertEqual(
            self.client.get("/DeepFolder/Level1/Level2/l2_note.md").status_code, 404
        )

    def test_08_create_folder_with_spaces(self):
        """Create a folder with spaces in the name."""
        resp = self.client.mkcol("/My Folder Name")
        self.assertIn(resp.status_code, [200, 201])

        # Create a note in it
        content = "# In spaced folder"
        resp = self.client.put("/My Folder Name/test.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

        # Read it back
        resp = self.client.get("/My Folder Name/test.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

        # Clean up
        self.client.delete("/My Folder Name")

    def test_09_folder_already_exists(self):
        """Creating a folder that already exists should return error."""
        self.client.mkcol("/DuplicateFolder")
        resp = self.client.mkcol("/DuplicateFolder")
        # Should return 405 Method Not Allowed or 409 Conflict
        self.assertIn(resp.status_code, [405, 409])

        # Clean up
        self.client.delete("/DuplicateFolder")

    def test_10_create_note_in_nonexistent_folder(self):
        """Creating a note in a non-existent folder should fail with 409 Conflict.

        Per WebDAV RFC 4918: 409 Conflict is returned when creating a resource
        where not all ancestors of the destination resource exist (the
        'ancestor-does-not-exist' precondition violation).
        """
        resp = self.client.put("/NonExistentFolder12345/test.md", "content")
        self.assertEqual(resp.status_code, 409)  # 409 Conflict per WebDAV spec


class TestEdgeCases(unittest.TestCase):
    """Test edge cases and error handling."""

    @classmethod
    def setUpClass(cls):
        cls.client = WebDAVClient(BASE_URL, AUTH)

    def test_01_empty_content(self):
        """Create a note with empty content."""
        resp = self.client.put("/empty_note.md", "")
        self.assertIn(resp.status_code, [200, 201, 204])

        resp = self.client.get("/empty_note.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, "")

        # Clean up
        self.client.delete("/empty_note.md")

    def test_02_special_characters_in_content(self):
        """Create a note with special characters."""
        content = "Special chars: <>&\"'`~!@#$%^&*()[]{}|\\:;,.<>?"
        resp = self.client.put("/special_chars.md", content)
        self.assertIn(resp.status_code, [200, 201, 204])

        resp = self.client.get("/special_chars.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

        # Clean up
        self.client.delete("/special_chars.md")

    def test_03_url_encoded_path(self):
        """Test URL-encoded paths work correctly."""
        content = "# URL Encoded"
        # Using explicit URL encoding for spaces
        path = "/URL%20Encoded%20Note.md"
        resp = self.client.put(path, content)
        self.assertIn(resp.status_code, [200, 201, 204])

        resp = self.client.get(path)
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, content)

        # Clean up
        self.client.delete(path)

    def test_04_trailing_slashes(self):
        """Test that trailing slashes are handled correctly."""
        # Create folder
        self.client.mkcol("/TrailingSlashTest")
        self.client.put("/TrailingSlashTest/note.md", "content")

        # PROPFIND with and without trailing slash should work
        resp1 = self.client.propfind("/TrailingSlashTest")
        resp2 = self.client.propfind("/TrailingSlashTest/")
        self.assertEqual(resp1.status_code, 207)
        self.assertEqual(resp2.status_code, 207)

        # Clean up
        self.client.delete("/TrailingSlashTest")

    def test_05_delete_root_forbidden(self):
        """Deleting root should be forbidden."""
        resp = self.client.delete("/")
        self.assertIn(resp.status_code, [403, 405])

    def test_06_propfind_depth_0(self):
        """PROPFIND with Depth: 0 should only return the resource itself."""
        self.client.mkcol("/DepthTest")
        self.client.put("/DepthTest/note1.md", "content")

        resp = self.client.propfind("/DepthTest", depth=0)
        self.assertEqual(resp.status_code, 207)
        # Should only contain info about the folder, not children
        # The response should have limited href entries
        import re

        hrefs = re.findall(r"<D:href>([^<]+)</D:href>", resp.text)
        self.assertEqual(len(hrefs), 1)  # Only the folder itself

        # Clean up
        self.client.delete("/DepthTest")

    def test_07_head_request(self):
        """HEAD request should return metadata without body."""
        self.client.put("/head_test.md", "Some content here")

        session = requests.Session()
        session.auth = AUTH
        resp = session.head(BASE_URL + "/head_test.md")
        self.assertEqual(resp.status_code, 200)
        self.assertEqual(resp.text, "")  # No body

        # Clean up
        self.client.delete("/head_test.md")

    def test_08_concurrent_updates(self):
        """Test concurrent updates to the same note."""
        import concurrent.futures

        path = "/concurrent_test.md"
        self.client.put(path, "initial")

        def update(n):
            return self.client.put(path, f"update {n}")

        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as executor:
            futures = [executor.submit(update, i) for i in range(5)]
            results = [f.result() for f in futures]

        # All should succeed
        for resp in results:
            self.assertIn(resp.status_code, [200, 201, 204])

        # Clean up
        self.client.delete(path)


class TestCleanup(unittest.TestCase):
    """Clean up test data after all tests."""

    @classmethod
    def setUpClass(cls):
        cls.client = WebDAVClient(BASE_URL, AUTH)

    def test_cleanup(self):
        """Clean up any remaining test files and folders."""
        # List of paths to clean up
        cleanup_paths = [
            "/test_create.md",
            "/test_read.md",
            "/test_update.md",
            "/TestFolder",
            "/ListTestFolder",
            "/NoteTestFolder",
            "/ParentFolder",
        ]

        for path in cleanup_paths:
            try:
                self.client.delete(path)
            except Exception:
                pass  # Ignore errors during cleanup


def run_server_fixture():
    """
    Helper to start the server for testing.
    Returns the process handle.
    """
    # Create test database
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write(
            """
CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  parent_id TEXT REFERENCES folders(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    syntax TEXT NOT NULL DEFAULT 'md',
    parent_id TEXT REFERENCES folders(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
"""
        )
        sql_file = f.name

    db_file = tempfile.mktemp(suffix=".db")
    subprocess.run(["sqlite3", db_file], stdin=open(sql_file), check=True)

    # Start server
    proc = subprocess.Popen(
        [
            "./target/release/webdav_server",
            "serve",
            "-d",
            db_file,
            "-u",
            USERNAME,
            "-P",
            PASSWORD,
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    # Wait for server to be ready
    time.sleep(2)
    return proc, db_file


if __name__ == "__main__":
    print(f"Testing WebDAV server at {BASE_URL}")
    print(f"Username: {USERNAME}")
    print()

    # Check if server is running
    try:
        resp = requests.get(BASE_URL + "/", auth=AUTH, timeout=2)
        print(f"Server is running (status: {resp.status_code})")
    except requests.exceptions.ConnectionError:
        print("ERROR: Server is not running!")
        print(f"Please start the server first:")
        print(f"  ./target/release/webdav_server serve -d <database> -u {USERNAME} -P {PASSWORD}")
        exit(1)

    print()
    print("=" * 60)
    print()

    # Run tests
    unittest.main(verbosity=2)
