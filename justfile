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

serve:
    cargo run --profile=release  -- serve --database mydatabase.sqlite  -u ryan -P 1234 --user-id 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' --host '0.0.0.0'


# Testing
serve-for-test:
    cargo run --profile=release  -- serve --database mydatabase.sqlite  -u testuser -P testpass --user-id 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' --host '0.0.0.0'

run-test:
    python ./tests/test_webdav.py
