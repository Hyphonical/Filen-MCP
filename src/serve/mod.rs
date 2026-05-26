//! MCP server bootstrap for Filen-MCP.
//!
//! Reads the persisted auth config from disk (or `FILEN_AUTH_CONFIG_JSON` env
//! var), reconstructs the Filen `Client`, and starts the MCP server over stdio.

use std::sync::Arc;

use anyhow::Context as _;
use filen_sdk_rs::auth::http::ClientConfig;
use filen_sdk_rs::auth::unauth::UnauthClient;
use filen_sdk_rs::auth::{Client, StringifiedClient};
use rmcp::ServiceExt;
use tokio::sync::Mutex;

use crate::config::auth_config_path;
use crate::tools::FilenMcpServer;

// ---------------------------------------------------------------------------
// Shared State
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SharedState {
	pub client: Arc<Mutex<Option<Client>>>,
	pub unauth: Arc<UnauthClient>,
	pub email: String,
	pub user_id: u64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
	let config_json = read_auth_config()?;
	let state = build_state(&config_json)
		.context("Failed to construct Filen client from config. Run 'filen-mcp login' first.")?;

	eprintln!("Loaded auth config");
	eprintln!("Filen-MCP server started (stdio)");
	eprintln!("Connected as {}", state.email);

	let server = FilenMcpServer::new(state);
	server
		.serve(rmcp::transport::stdio())
		.await
		.context("MCP server encountered an irrecoverable error")?;

	Ok(())
}

// ---------------------------------------------------------------------------
// Auth Config Loading
// ---------------------------------------------------------------------------

fn read_auth_config() -> anyhow::Result<String> {
	if let Ok(json) = std::env::var("FILEN_AUTH_CONFIG_JSON")
		&& !json.trim().is_empty()
	{
		return Ok(json);
	}

	let path = auth_config_path()?;

	std::fs::read_to_string(&path).with_context(|| {
		format!(
			"Failed to read auth config at: {}. Run 'filen-mcp login' first.",
			path.display()
		)
	})
}

fn build_state(config_json: &str) -> anyhow::Result<SharedState> {
	let stringified: StringifiedClient = serde_json::from_str(config_json)?;
	let email = stringified.email.clone();
	let user_id = stringified.user_id;
	let unauth = UnauthClient::from_config(ClientConfig::default())?;
	let client = unauth.from_stringified(stringified)?;
	Ok(SharedState {
		client: Arc::new(Mutex::new(Some(client))),
		unauth: Arc::new(unauth),
		email,
		user_id,
	})
}
