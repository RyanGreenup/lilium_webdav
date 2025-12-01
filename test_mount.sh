#!/bin/bash

# Test script for WebDAV file operations

# Create a mount point
MOUNT_POINT="/tmp/webdav_test"
mkdir -p "$MOUNT_POINT"

echo "Mounting WebDAV at $MOUNT_POINT..."
# Adjust the mount command based on your system
# For Linux with davfs2:
# sudo mount -t davfs http://127.0.0.1:4918/ "$MOUNT_POINT"

# For macOS:
# mount_webdav -i http://127.0.0.1:4918/ "$MOUNT_POINT"

echo "Mount point created at: $MOUNT_POINT"
echo ""
echo "To mount manually:"
echo "  Linux (davfs2): sudo mount -t davfs http://127.0.0.1:4918/ $MOUNT_POINT"
echo "  macOS: mount_webdav -i http://127.0.0.1:4918/ $MOUNT_POINT"
echo ""
echo "Then try:"
echo "  cd $MOUNT_POINT"
echo "  touch newfile.md"
echo ""
echo "To unmount:"
echo "  Linux: sudo umount $MOUNT_POINT"
echo "  macOS: umount $MOUNT_POINT"
