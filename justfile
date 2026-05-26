# Filen-MCP build orchestration
# https://github.com/casey/just
# Install `just` via: cargo install just

set windows-shell := ["pwsh.exe", "-NoLogo", "-Command"]
set shell := ["bash", "-uc"]

default: dev

# ── Build ──────────────────────────────────────────────────────────

dev:
	cargo fmt --all --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo build

release:
	cargo fmt --all --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo build --release

check:
	cargo fmt --all --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo check

# ── Quality ────────────────────────────────────────────────────────

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo fmt --all --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test

# ── Run ────────────────────────────────────────────────────────────

login:
	cargo run -- login

serve:
	cargo run -- serve
