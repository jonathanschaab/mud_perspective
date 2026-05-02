set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

default: fmt clippy test

# Format the codebase
fmt:
    cargo fmt

# Run tests against all feature combinations
test:
    cargo test --no-default-features
    cargo test --no-default-features --features "ansi"
    cargo test --no-default-features --features "mxp"
    cargo test --no-default-features --features "msp"
    cargo test --no-default-features --features "ansi mxp"
    cargo test --no-default-features --features "ansi msp"
    cargo test --no-default-features --features "mxp msp"
    cargo test --all-features

# Run clippy against all feature combinations
clippy:
    cargo clippy --no-default-features
    cargo clippy --no-default-features --features "ansi"
    cargo clippy --no-default-features --features "mxp"
    cargo clippy --no-default-features --features "msp"
    cargo clippy --no-default-features --features "ansi mxp"
    cargo clippy --no-default-features --features "ansi msp"
    cargo clippy --no-default-features --features "mxp msp"
    cargo clippy --all-features
