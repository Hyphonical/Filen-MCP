//! MCP tool implementations for Filen-MCP.
//!
//! All 17 tools that the MCP server exposes. Each tool delegates to the Filen
//! SDK for the actual cloud storage operations. File data never travels through
//! the JSON-RPC channel — only path strings do.

use std::borrow::Cow;
use std::sync::Arc;

use filen_sdk_rs::auth::Client;
use filen_sdk_rs::fs::HasName;
use filen_sdk_rs::fs::HasParent;
use filen_sdk_rs::fs::HasRemoteInfo;
use filen_sdk_rs::fs::HasUUID;
use filen_sdk_rs::fs::categories::DirType;
use filen_sdk_rs::fs::categories::NonRootFileType;
use filen_sdk_rs::fs::categories::NonRootItemType;
use filen_sdk_rs::fs::file::read::FileReaderBuilder;
use filen_sdk_rs::fs::file::traits::HasRemoteFileInfo;
use filen_sdk_rs::io::HasFileInfo;
use filen_types::fs::UuidStr;
use futures_util::io::AsyncWriteExt;
use rmcp::ErrorData;
use rmcp::Json;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use serde::{Deserialize, Serialize};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::serve::SharedState;

// ---------------------------------------------------------------------------
// MCP Server Handler
// ---------------------------------------------------------------------------

pub struct FilenMcpServer {
	state: Arc<SharedState>,
}

impl FilenMcpServer {
	pub fn new(state: SharedState) -> Self {
		Self {
			state: Arc::new(state),
		}
	}

	async fn client(&self) -> Result<tokio::sync::MutexGuard<'_, Option<Client>>, ErrorData> {
		let guard = self.state.client.lock().await;
		if guard.is_none() {
			return Err(ErrorData::internal_error(
				"Not authenticated. Run 'filen-mcp login' first.",
				None,
			));
		}
		Ok(guard)
	}
}

// ---------------------------------------------------------------------------
// Parameter & Response Types
// ---------------------------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
struct LsParams {
	path: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct DirEntry {
	name: String,
	uuid: String,
	#[serde(rename = "type")]
	entry_type: String,
	size: u64,
	mime: String,
	created: String,
	modified: String,
	favorited: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MkdirParams {
	path: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct MkdirOutput {
	uuid: String,
	name: String,
	created: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct UploadParams {
	local_path: String,
	remote_parent: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct UploadOutput {
	uuid: String,
	name: String,
	size: u64,
	remote_path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DownloadParams {
	remote_path: String,
	local_dest: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct DownloadOutput {
	size: u64,
	local_path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteParams {
	path: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct DeleteOutput {
	success: bool,
	path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MvParams {
	from: String,
	to: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct MvOutput {
	from: String,
	to: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct StatParams {
	path: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct StatOutput {
	name: String,
	uuid: String,
	#[serde(rename = "type")]
	entry_type: String,
	size: u64,
	mime: String,
	created: String,
	modified: String,
	parent: String,
	region: String,
	chunks: u64,
	favorited: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchParams {
	query: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SearchEntry {
	name: String,
	#[serde(rename = "type")]
	entry_type: String,
	uuid: String,
	path: String,
	size: u64,
}

#[derive(Serialize, schemars::JsonSchema)]
struct WhoamiOutput {
	email: String,
	user_id: u64,
}

#[derive(Serialize, schemars::JsonSchema)]
struct NoteEntry {
	uuid: String,
	title: String,
	preview: String,
	#[serde(rename = "type")]
	note_type: String,
	favorited: bool,
	pinned: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct NoteGetParams {
	uuid: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct NoteDetail {
	uuid: String,
	title: String,
	content: String,
	preview: String,
	#[serde(rename = "type")]
	note_type: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct NoteCreateParams {
	title: Option<String>,
	content: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct NoteCreateOutput {
	uuid: String,
	title: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct NoteUpdateParams {
	uuid: String,
	title: String,
	content: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct NoteDeleteParams {
	uuid: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct TrashEntry {
	name: String,
	uuid: String,
	#[serde(rename = "type")]
	entry_type: String,
	size: u64,
}

// ---------------------------------------------------------------------------
// Tool Implementations
// ---------------------------------------------------------------------------

#[tool_router(server_handler)]
impl FilenMcpServer {
	// ── filen_ls ──────────────────────────────────────────────────────

	#[tool(
		name = "filen_ls",
		description = "List contents of a remote Filen directory"
	)]
	async fn filen_ls(
		&self,
		Parameters(LsParams { path }): Parameters<LsParams>,
	) -> Result<Json<Vec<DirEntry>>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let item = resolve_path(client, &path).await?;

		let dir = match item {
			NonRootFileType::Dir(d) => d.into_owned(),
			_ => {
				return Err(ErrorData::internal_error(
					format!("Path is not a directory: {path}"),
					None,
				));
			}
		};

		let dir_type = DirType::Dir(Cow::Borrowed(&dir));

		let (dirs, files) = client
			.list_dir::<_, filen_sdk_rs::fs::categories::Normal>(
				&dir_type,
				None::<&fn(u64, Option<u64>)>,
			)
			.await
			.map_err(|e| {
				ErrorData::internal_error(format!("Failed to list directory: {e}"), None)
			})?;

		let mut entries = Vec::new();
		for d in dirs {
			entries.push(DirEntry {
				name: d.name().unwrap_or("?").to_string(),
				uuid: d.uuid().to_string(),
				entry_type: "directory".to_string(),
				size: 0,
				mime: String::new(),
				created: format_opt_dt(Some(d.timestamp())),
				modified: String::new(),
				favorited: d.favorited(),
			});
		}
		for f in files {
			entries.push(DirEntry {
				name: f.name().unwrap_or("?").to_string(),
				uuid: f.uuid().to_string(),
				entry_type: "file".to_string(),
				size: f.size(),
				mime: f.mime().unwrap_or("").to_string(),
				created: format_opt_dt(f.created()),
				modified: format_opt_dt(f.last_modified()),
				favorited: f.favorited(),
			});
		}

		Ok(Json(entries))
	}

	// ── filen_mkdir ───────────────────────────────────────────────────

	#[tool(
		name = "filen_mkdir",
		description = "Create a remote directory on Filen"
	)]
	async fn filen_mkdir(
		&self,
		Parameters(MkdirParams { path }): Parameters<MkdirParams>,
	) -> Result<Json<MkdirOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let (parent_path, name) = split_path(&path)
			.ok_or_else(|| ErrorData::internal_error(format!("Invalid path: {path}"), None))?;

		let item = resolve_path(client, parent_path).await?;

		let parent = match item {
			NonRootFileType::Dir(d) => d.into_owned(),
			_ => {
				return Err(ErrorData::internal_error(
					format!("Parent is not a directory: {parent_path}"),
					None,
				));
			}
		};

		let parent_type = DirType::Dir(Cow::Borrowed(&parent));

		let dir = client.create_dir(&parent_type, name).await.map_err(|e| {
			ErrorData::internal_error(format!("Failed to create directory: {e}"), None)
		})?;

		Ok(Json(MkdirOutput {
			uuid: dir.uuid().to_string(),
			name: dir.name().unwrap_or("?").to_string(),
			created: format_opt_dt(Some(dir.timestamp())),
		}))
	}

	// ── filen_upload ──────────────────────────────────────────────────

	#[tool(
		name = "filen_upload",
		description = "Upload a local file to a remote Filen directory"
	)]
	async fn filen_upload(
		&self,
		Parameters(UploadParams {
			local_path,
			remote_parent,
		}): Parameters<UploadParams>,
	) -> Result<Json<UploadOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let file_name = std::path::Path::new(&local_path)
			.file_name()
			.and_then(|n| n.to_str())
			.ok_or_else(|| ErrorData::internal_error("Invalid local path", None))?;

		let item = resolve_path(client, &remote_parent).await?;

		let dir = match item {
			NonRootFileType::Dir(d) => d.into_owned(),
			_ => {
				return Err(ErrorData::internal_error(
					format!("Remote path is not a directory: {remote_parent}"),
					None,
				));
			}
		};

		let builder = client
			.make_file_builder(file_name, *dir.uuid())
			.map_err(|e| ErrorData::internal_error(format!("Failed to build file: {e}"), None))?;

		let mut writer = client.get_file_writer(builder);

		let data = tokio::fs::read(&local_path)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Cannot open local file: {e}"), None))?;

		writer
			.write_all(&data)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Upload failed: {e}"), None))?;

		writer
			.close()
			.await
			.map_err(|e| ErrorData::internal_error(format!("Upload finalize failed: {e}"), None))?;

		let remote_file = writer
			.into_remote_file()
			.ok_or_else(|| ErrorData::internal_error("Upload did not return a file", None))?;

		Ok(Json(UploadOutput {
			uuid: remote_file.uuid().to_string(),
			name: remote_file.name().unwrap_or("unknown").to_string(),
			size: remote_file.size(),
			remote_path: format!("/{}/{}", remote_parent.trim_matches('/'), file_name),
		}))
	}

	// ── filen_download ────────────────────────────────────────────────

	#[tool(
		name = "filen_download",
		description = "Download a remote Filen file to a local path"
	)]
	async fn filen_download(
		&self,
		Parameters(DownloadParams {
			remote_path,
			local_dest,
		}): Parameters<DownloadParams>,
	) -> Result<Json<DownloadOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let item = resolve_path(client, &remote_path).await?;

		let file = match item {
			NonRootFileType::File(f) => f.into_owned(),
			_ => {
				return Err(ErrorData::internal_error(
					format!("Path is not a file: {remote_path}"),
					None,
				));
			}
		};

		let reader = FileReaderBuilder::new(&self.state.unauth, &file).build();
		let mut compat_reader = reader.compat();

		let mut output = tokio::fs::File::create(&local_dest).await.map_err(|e| {
			ErrorData::internal_error(format!("Cannot create local file: {e}"), None)
		})?;

		let bytes = tokio::io::copy(&mut compat_reader, &mut output)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Download failed: {e}"), None))?;

		Ok(Json(DownloadOutput {
			size: bytes,
			local_path: local_dest,
		}))
	}

	// ── filen_delete ──────────────────────────────────────────────────

	#[tool(
		name = "filen_delete",
		description = "Move a file or directory to Filen trash"
	)]
	async fn filen_delete(
		&self,
		Parameters(DeleteParams { path }): Parameters<DeleteParams>,
	) -> Result<Json<DeleteOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let item = resolve_path(client, &path).await?;

		match item {
			NonRootFileType::Dir(mut d) => {
				client.trash_dir(d.to_mut()).await.map_err(|e| {
					ErrorData::internal_error(format!("Failed to trash directory: {e}"), None)
				})?;
			}
			NonRootFileType::File(mut f) => {
				client.trash_file(f.to_mut()).await.map_err(|e| {
					ErrorData::internal_error(format!("Failed to trash file: {e}"), None)
				})?;
			}
			_ => {
				return Err(ErrorData::internal_error(
					format!("Cannot trash root: {path}"),
					None,
				));
			}
		}

		Ok(Json(DeleteOutput {
			success: true,
			path,
		}))
	}

	// ── filen_mv ──────────────────────────────────────────────────────

	#[tool(
		name = "filen_mv",
		description = "Move a file or directory to another location"
	)]
	async fn filen_mv(
		&self,
		Parameters(MvParams { from, to }): Parameters<MvParams>,
	) -> Result<Json<MvOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let src_item = resolve_path(client, &from).await?;
		let dest_item = resolve_path(client, &to).await?;

		let dest_dir = match dest_item {
			NonRootFileType::Dir(d) => d.into_owned(),
			_ => {
				return Err(ErrorData::internal_error(
					format!("Destination is not a directory: {to}"),
					None,
				));
			}
		};

		let dest_type = DirType::Dir(Cow::Borrowed(&dest_dir));

		match src_item {
			NonRootFileType::File(mut f) => {
				client
					.move_file(f.to_mut(), &dest_type)
					.await
					.map_err(|e| {
						ErrorData::internal_error(format!("Failed to move file: {e}"), None)
					})?;
			}
			NonRootFileType::Dir(mut d) => {
				client.move_dir(d.to_mut(), &dest_type).await.map_err(|e| {
					ErrorData::internal_error(format!("Failed to move directory: {e}"), None)
				})?;
			}
			_ => {
				return Err(ErrorData::internal_error(
					format!("Cannot move root: {from}"),
					None,
				));
			}
		}

		Ok(Json(MvOutput { from, to }))
	}

	// ── filen_stat ────────────────────────────────────────────────────

	#[tool(
		name = "filen_stat",
		description = "Get metadata for a remote file or directory"
	)]
	async fn filen_stat(
		&self,
		Parameters(StatParams { path }): Parameters<StatParams>,
	) -> Result<Json<StatOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let item = resolve_path(client, &path).await?;

		let (
			name,
			uuid,
			entry_type,
			size,
			mime,
			created,
			modified,
			favorited,
			parent,
			region,
			chunks,
		) = match item {
			NonRootFileType::Dir(ref d) => (
				d.name().unwrap_or("?").to_string(),
				d.uuid().to_string(),
				"directory",
				0u64,
				String::new(),
				format_opt_dt(Some(d.timestamp())),
				String::new(),
				d.favorited(),
				String::new(),
				String::new(),
				0u64,
			),
			NonRootFileType::File(ref f) => (
				f.name().unwrap_or("?").to_string(),
				f.uuid().to_string(),
				"file",
				f.size(),
				f.mime().unwrap_or("").to_string(),
				format_opt_dt(f.created()),
				format_opt_dt(f.last_modified()),
				f.favorited(),
				f.parent().to_string(),
				f.region().to_string(),
				f.chunks(),
			),
			_ => {
				return Err(ErrorData::internal_error(
					format!("Stat not supported for root items: {path}"),
					None,
				));
			}
		};

		Ok(Json(StatOutput {
			name,
			uuid,
			entry_type: entry_type.to_string(),
			size,
			mime,
			created,
			modified,
			parent,
			region,
			chunks,
			favorited,
		}))
	}

	// ── filen_search ──────────────────────────────────────────────────

	#[tool(
		name = "filen_search",
		description = "Search files and directories by name"
	)]
	async fn filen_search(
		&self,
		Parameters(SearchParams { query }): Parameters<SearchParams>,
	) -> Result<Json<Vec<SearchEntry>>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let matches = client
			.find_item_matches_for_name(&query)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Search failed: {e}"), None))?;

		let results = matches
			.into_iter()
			.map(|(item, item_path)| {
				let (name, entry_type, uuid, size) = match &item {
					NonRootItemType::Dir(d) => (
						d.name().unwrap_or("?").to_string(),
						"directory",
						d.uuid().to_string(),
						0u64,
					),
					NonRootItemType::File(f) => (
						f.name().unwrap_or("?").to_string(),
						"file",
						f.uuid().to_string(),
						f.size(),
					),
				};
				SearchEntry {
					name,
					entry_type: entry_type.to_string(),
					uuid,
					path: item_path,
					size,
				}
			})
			.collect();

		Ok(Json(results))
	}

	// ── filen_whoami ──────────────────────────────────────────────────

	#[tool(
		name = "filen_whoami",
		description = "Get current authenticated user info"
	)]
	async fn filen_whoami(&self) -> Result<Json<WhoamiOutput>, ErrorData> {
		Ok(Json(WhoamiOutput {
			email: self.state.email.clone(),
			user_id: self.state.user_id,
		}))
	}

	// ── filen_notes_list ──────────────────────────────────────────────

	#[tool(name = "filen_notes_list", description = "List all notes")]
	async fn filen_notes_list(&self) -> Result<Json<Vec<NoteEntry>>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let notes = client
			.list_notes()
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to list notes: {e}"), None))?;

		let results = notes
			.into_iter()
			.map(|n| NoteEntry {
				uuid: n.uuid().to_string(),
				title: n.title().unwrap_or("").to_string(),
				preview: n.preview().unwrap_or("").to_string(),
				note_type: note_type_str(n.note_type()),
				favorited: n.favorited(),
				pinned: n.pinned(),
			})
			.collect();

		Ok(Json(results))
	}

	// ── filen_note_get ────────────────────────────────────────────────

	#[tool(
		name = "filen_note_get",
		description = "Get a note by UUID including its full content"
	)]
	async fn filen_note_get(
		&self,
		Parameters(NoteGetParams { uuid }): Parameters<NoteGetParams>,
	) -> Result<Json<NoteDetail>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let note_uuid = uuid::Uuid::parse_str(&uuid)
			.map_err(|e| ErrorData::internal_error(format!("Invalid UUID: {e}"), None))?;

		let mut note = client
			.get_note(UuidStr::from(&note_uuid))
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to get note: {e}"), None))?
			.ok_or_else(|| ErrorData::internal_error(format!("Note not found: {uuid}"), None))?;

		let content = client
			.get_note_content(&mut note)
			.await
			.map_err(|e| {
				ErrorData::internal_error(format!("Failed to get note content: {e}"), None)
			})?
			.unwrap_or_default();

		Ok(Json(NoteDetail {
			uuid: note.uuid().to_string(),
			title: note.title().unwrap_or("").to_string(),
			content,
			preview: note.preview().unwrap_or("").to_string(),
			note_type: note_type_str(note.note_type()),
		}))
	}

	// ── filen_note_create ─────────────────────────────────────────────

	#[tool(
		name = "filen_note_create",
		description = "Create a new note with title and content"
	)]
	async fn filen_note_create(
		&self,
		Parameters(NoteCreateParams { title, content }): Parameters<NoteCreateParams>,
	) -> Result<Json<NoteCreateOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let preview: String = content.chars().take(100).collect();

		let mut note = client
			.create_note(title)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to create note: {e}"), None))?;

		client
			.set_note_content(&mut note, &content, preview)
			.await
			.map_err(|e| {
				ErrorData::internal_error(format!("Failed to set note content: {e}"), None)
			})?;

		Ok(Json(NoteCreateOutput {
			uuid: note.uuid().to_string(),
			title: note.title().unwrap_or("").to_string(),
		}))
	}

	// ── filen_note_update ─────────────────────────────────────────────

	#[tool(
		name = "filen_note_update",
		description = "Update a note's title and/or content"
	)]
	async fn filen_note_update(
		&self,
		Parameters(NoteUpdateParams {
			uuid,
			title,
			content,
		}): Parameters<NoteUpdateParams>,
	) -> Result<Json<NoteDetail>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let note_uuid = uuid::Uuid::parse_str(&uuid)
			.map_err(|e| ErrorData::internal_error(format!("Invalid UUID: {e}"), None))?;

		let mut note = client
			.get_note(UuidStr::from(&note_uuid))
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to get note: {e}"), None))?
			.ok_or_else(|| ErrorData::internal_error(format!("Note not found: {uuid}"), None))?;

		client
			.set_note_title(&mut note, title)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to update title: {e}"), None))?;

		if let Some(content) = content {
			let preview: String = content.chars().take(100).collect();
			client
				.set_note_content(&mut note, &content, preview)
				.await
				.map_err(|e| {
					ErrorData::internal_error(format!("Failed to update content: {e}"), None)
				})?;
		}

		let updated_content = client
			.get_note_content(&mut note)
			.await
			.map_err(|e| {
				ErrorData::internal_error(format!("Failed to get note content: {e}"), None)
			})?
			.unwrap_or_default();

		Ok(Json(NoteDetail {
			uuid: note.uuid().to_string(),
			title: note.title().unwrap_or("").to_string(),
			content: updated_content,
			preview: note.preview().unwrap_or("").to_string(),
			note_type: note_type_str(note.note_type()),
		}))
	}

	// ── filen_note_delete ─────────────────────────────────────────────

	#[tool(name = "filen_note_delete", description = "Delete a note by UUID")]
	async fn filen_note_delete(
		&self,
		Parameters(NoteDeleteParams { uuid }): Parameters<NoteDeleteParams>,
	) -> Result<Json<DeleteOutput>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let note_uuid = uuid::Uuid::parse_str(&uuid)
			.map_err(|e| ErrorData::internal_error(format!("Invalid UUID: {e}"), None))?;

		let note = client
			.get_note(UuidStr::from(&note_uuid))
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to get note: {e}"), None))?
			.ok_or_else(|| ErrorData::internal_error(format!("Note not found: {uuid}"), None))?;

		client
			.delete_note(note)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to delete note: {e}"), None))?;

		Ok(Json(DeleteOutput {
			success: true,
			path: uuid,
		}))
	}

	// ── filen_shares_in ───────────────────────────────────────────────

	#[tool(name = "filen_shares_in", description = "List items shared with you")]
	async fn filen_shares_in(&self) -> Result<Json<serde_json::Value>, ErrorData> {
		Err(ErrorData::internal_error(
			"Share listing is not yet implemented. The SDK does not currently expose a public API for listing inbound shares.",
			None,
		))
	}

	// ── filen_shares_out ──────────────────────────────────────────────

	#[tool(name = "filen_shares_out", description = "List items you have shared")]
	async fn filen_shares_out(&self) -> Result<Json<serde_json::Value>, ErrorData> {
		Err(ErrorData::internal_error(
			"Share listing is not yet implemented. The SDK does not currently expose a public API for listing outbound shares.",
			None,
		))
	}

	// ── filen_ls_trash ────────────────────────────────────────────────

	#[tool(
		name = "filen_ls_trash",
		description = "List items in your Filen trash"
	)]
	async fn filen_ls_trash(&self) -> Result<Json<Vec<TrashEntry>>, ErrorData> {
		let guard = self.client().await?;
		let client = guard.as_ref().unwrap();

		let (dirs, files) = client
			.list_trash(None::<&fn(u64, Option<u64>)>)
			.await
			.map_err(|e| ErrorData::internal_error(format!("Failed to list trash: {e}"), None))?;

		let mut results = Vec::new();
		for d in dirs {
			results.push(TrashEntry {
				name: d.name().unwrap_or("?").to_string(),
				uuid: d.uuid().to_string(),
				entry_type: "directory".to_string(),
				size: 0,
			});
		}
		for f in files {
			results.push(TrashEntry {
				name: f.name().unwrap_or("?").to_string(),
				uuid: f.uuid().to_string(),
				entry_type: "file".to_string(),
				size: f.size(),
			});
		}

		Ok(Json(results))
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve a remote path to a file or directory item.
///
/// Delegates to the SDK's HMAC-based path resolution. Returns the item if
/// found, or an `ErrorData` describing the failure otherwise.
///
/// # Errors
/// Returns an internal error if the path is not found or resolution fails.
async fn resolve_path<'a>(
	client: &'a Client,
	path: &str,
) -> Result<NonRootFileType<'a, filen_sdk_rs::fs::categories::Normal>, ErrorData> {
	client
		.find_item_at_path(path)
		.await
		.map_err(|e| ErrorData::internal_error(format!("Path not found: {path}: {e}"), None))?
		.ok_or_else(|| ErrorData::internal_error(format!("Path not found: {path}"), None))
}

/// Split a path string into `(parent, name)` at the last `/`.
///
/// Returns `None` if the path has no separator (i.e. is just a name).
fn split_path(path: &str) -> Option<(&str, &str)> {
	let path = path.trim_matches('/');
	let idx = path.rfind('/')?;
	Some((&path[..idx], &path[idx + 1..]))
}

/// Format an optional `DateTime<Utc>` as an RFC 3339 string,
/// returning an empty string for `None`.
fn format_opt_dt(opt: Option<chrono::DateTime<chrono::Utc>>) -> String {
	opt.map(|dt| dt.to_rfc3339()).unwrap_or_default()
}

/// Convert a note type to its string representation.
///
/// `filen_types::api::v3::notes::NoteType` does not implement `Display`,
/// but derives `Debug` with clean variant names (`Text`, `Md`, `Code`,
/// `Rich`, `Checklist`), so the Debug output is suitable for the MCP
/// tool response.
fn note_type_str(nt: impl std::fmt::Debug) -> String {
	format!("{:?}", nt)
}
