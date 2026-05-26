//! Interactive login flow for Filen authentication.
//!
//! Prompts the user for email, password, and optional 2FA code, then persists
//! the resulting `StringifiedClient` to a config file on disk. The stored config
//! is reused by `serve::run()` on subsequent starts — no password or 2FA code
//! is needed again.

use std::io::{self, Write};

use anyhow::Context as _;
use filen_sdk_rs::auth::http::ClientConfig;
use filen_sdk_rs::auth::unauth::UnauthClient;

use crate::config::auth_config_path;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
	let email = prompt("Email")?;
	let password = rpassword::prompt_password("Password: ")?;
	let two_factor_code_raw =
		rpassword::prompt_password("Two-factor code (leave blank if none): ")?;

	let two_factor_code = if two_factor_code_raw.is_empty() {
		None
	} else {
		Some(two_factor_code_raw)
	};

	let unauth = UnauthClient::from_config(ClientConfig::default())
		.context("Failed to create unauth client")?;

	println!("Logging in as {email}...");
	let client = unauth
		.login(email, &password, &two_factor_code.unwrap_or_default())
		.await
		.context("Login failed — check your credentials")?;

	let stringified = client.to_stringified();
	let config_path = auth_config_path()?;
	if let Some(parent) = config_path.parent() {
		std::fs::create_dir_all(parent).context("Failed to create config directory")?;
	}
	std::fs::write(&config_path, serde_json::to_string_pretty(&stringified)?)
		.context("Failed to write auth config")?;

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))?;
	}

	println!("Authenticated — config saved to {}", config_path.display());
	Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn prompt(label: &str) -> io::Result<String> {
	let mut stdout = io::stdout();
	write!(stdout, "{label}: ")?;
	stdout.flush()?;

	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	Ok(input.trim().to_string())
}
