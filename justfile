set dotenv-load

# Default recipe: list available commands
default:
    @just --list

# Format code
fmt:
    cargo clippy --fix
    cargo fmt

check:
    cargo check
    cargo test

clippy: check
    cargo clippy --all-targets --fix

mount:
    doas umount -l /home/ryan/Downloads/testing_webdav; doas mount -t davfs -o username=ryan http://localhost:4918 ~/Downloads/testing_webdav

# Force unmount a davfs2 mount (kills any processes using it first)
unmount path:
    -fuser -km {{path}}
    umount -l {{path}}

serve:
    cargo run --profile=release -- serve --database "${WEBDAV_DATABASE}" --host "${WEBDAV_HOST}" --port "${WEBDAV_PORT}" --username "${WEBDAV_USERNAME}" --password "${WEBDAV_PASSWORD}" --user-id "${WEBDAV_USER_ID}"


# Testing
serve-for-test:
    cargo run --profile=release  -- serve --database ./tests/fixtures/test_db.sqlite  -u testuser -P testpass --user-id 'testuserId' --host '0.0.0.0'

run-test:
    python ./tests/test_webdav.py
