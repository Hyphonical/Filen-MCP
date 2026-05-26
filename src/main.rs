//! Filen-MCP — MCP server for Filen cloud storage.
//!
//! A local MCP (Model Context Protocol) server that exposes Filen cloud storage
//! operations to LLM clients (Claude Desktop, Cursor, etc.). Runs entirely on
//! the local machine — no remote server, no file data over JSON-RPC.

mod config;
mod login;
mod serve;
mod tools;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "filen-mcp", about = "MCP server for Filen cloud storage")]
struct Cli {
	#[command(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	/// Interactive login — prompts for email, password, and 2FA code
	Login,
	/// Start the MCP server over stdio
	Serve,
}

// ---------------------------------------------------------------------------
// Entry Point
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
	tracing_subscriber::fmt::init();

	let cli = Cli::parse();

	let runtime = tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()?;

	match cli.command {
		Command::Login => runtime.block_on(login::run())?,
		Command::Serve => runtime.block_on(serve::run())?,
	}

	Ok(())
}
