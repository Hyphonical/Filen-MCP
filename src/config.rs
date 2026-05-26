//! Shared configuration helpers for Filen-MCP.
//!
//! Resolves the auth config file path and provides a unified default across
//! the login, serve, and any future subcommands that need the persisted
//! `StringifiedClient`.

use anyhow::Context as _;

/// Returns the path to the auth config file.
///
/// Respects the `FILEN_AUTH_CONFIG` environment variable if set, otherwise
/// falls back to a platform-appropriate default directory.
///
/// # Platform Defaults
/// - Windows: `%APPDATA%\filen-mcp\auth.json`
/// - Unix:    `$HOME/.config/filen-mcp/auth.json`
pub fn auth_config_path() -> anyhow::Result<std::path::PathBuf> {
	if let Ok(path) = std::env::var("FILEN_AUTH_CONFIG") {
		return Ok(std::path::PathBuf::from(path));
	}

	let base = app_config_dir()?;
	Ok(base.join("filen-mcp").join("auth.json"))
}

/// Returns the platform-appropriate application config directory.
fn app_config_dir() -> anyhow::Result<std::path::PathBuf> {
	#[cfg(target_os = "windows")]
	{
		std::env::var("APPDATA")
			.map(std::path::PathBuf::from)
			.context("APPDATA not set")
	}
	#[cfg(not(target_os = "windows"))]
	{
		std::env::var("HOME")
			.map(|h| std::path::PathBuf::from(h).join(".config"))
			.context("HOME not set")
	}
}
