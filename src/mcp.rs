//! Read-only MCP server. The process's startup directory is the access boundary.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::walker::{discover_files_with_filters, Discovery};
use crate::{compute_stats, estimate_tokens, format_markdown, skeletonize};
use crate::{FileResult, RepoStats};

const MAX_FILES: usize = 1_000;
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

fn default_path() -> String {
    ".".to_owned()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContextParams {
    /// File or directory relative to the server root. Defaults to the whole root.
    #[serde(default = "default_path")]
    pub path: String,
    /// Globs relative to the selected path; matching files keep their full source.
    #[serde(default)]
    pub focus: Vec<String>,
    /// Globs relative to the selected path; exclude matching files unless focused.
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Clone)]
pub struct SkelServer {
    root: PathBuf,
}

#[derive(Clone, Copy)]
enum Operation {
    List,
    Context,
}

impl SkelServer {
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|err| format!("Cannot open server root: {err}"))?;
        if !root.is_dir() {
            return Err("MCP server root must be a directory".to_owned());
        }
        Ok(Self { root })
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf, String> {
        let resolved = self
            .root
            .join(path)
            .canonicalize()
            .map_err(|err| format!("Cannot resolve '{}': {err}", path.display()))?;
        if !resolved.starts_with(&self.root) {
            return Err("Requested path is outside the MCP server root".to_owned());
        }
        Ok(resolved)
    }

    fn discover(&self, params: &ContextParams) -> Result<Discovery, String> {
        let selected = self.resolve(Path::new(&params.path))?;
        let mut discovery =
            discover_files_with_filters(&selected, &params.focus, &params.exclude, MAX_FILES)
                .map_err(|err| err.to_string())?;
        if discovery.files.is_empty() && !discovery.warnings.is_empty() {
            return Err(discovery.warnings.join("\n"));
        }
        // Check file links too: WalkDir does not follow directory links, but a
        // source file itself can be a symlink to a file outside the root.
        for file in &mut discovery.files {
            file.absolute_path = self.resolve(&file.absolute_path)?;
        }
        Ok(discovery)
    }

    async fn execute(&self, params: ContextParams, operation: Operation) -> CallToolResult {
        let server = self.clone();
        let result =
            tokio::task::spawn_blocking(move || server.execute_sync(params, operation)).await;
        match result {
            Ok(Ok(result)) => {
                let size = serde_json::to_vec(&result).map(|v| v.len());
                match size {
                    Ok(bytes) if bytes <= MAX_RESPONSE_BYTES => {
                        if matches!(operation, Operation::Context) {
                            if let Some(metadata) = &result.structured_content {
                                eprintln!("skel context summary: {metadata}");
                            }
                        }
                        result
                    }
                    Ok(_) => tool_error("Response exceeds 2 MiB. Select a smaller directory."),
                    Err(err) => tool_error(format!("Cannot serialize result: {err}")),
                }
            }
            Ok(Err(err)) => tool_error(err),
            Err(err) => tool_error(format!("Context worker failed: {err}")),
        }
    }

    fn execute_sync(
        &self,
        params: ContextParams,
        operation: Operation,
    ) -> Result<CallToolResult, String> {
        let Discovery {
            files,
            excluded,
            mut warnings,
        } = self.discover(&params)?;
        if matches!(operation, Operation::List) {
            let entries: Vec<_> = files
                .iter()
                .map(|file| {
                    json!({
                        "path": file.relative_path,
                        "language": file.lang,
                        "focus": file.is_focus,
                    })
                })
                .collect();
            return Ok(CallToolResult::structured(json!({
                "file_count": entries.len(), "files": entries, "warnings": warnings,
                "excluded_count": excluded.len(), "excluded_files": excluded,
            })));
        }

        let mut results = Vec::with_capacity(files.len());
        let mut input_bytes = 0;
        for file in files {
            let mut bytes = Vec::new();
            let read_result = File::open(&file.absolute_path).and_then(|reader| {
                reader
                    .take((MAX_FILE_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
            });
            if let Err(err) = read_result {
                warnings.push(format!("Skipped '{}': {err}", file.relative_path));
                continue;
            }
            if bytes.len() > MAX_FILE_BYTES {
                return Err(format!("File '{}' exceeds 2 MiB", file.relative_path));
            }
            input_bytes += bytes.len();
            if input_bytes > MAX_INPUT_BYTES {
                return Err("Source input exceeds 16 MiB. Select a smaller directory.".to_owned());
            }
            let source = match String::from_utf8(bytes) {
                Ok(source) => source,
                Err(_) => {
                    warnings.push(format!(
                        "Skipped '{}': source is not UTF-8",
                        file.relative_path
                    ));
                    continue;
                }
            };
            let orig_tokens = estimate_tokens(&source);
            let extension = Path::new(&file.relative_path)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");
            let content = if file.is_focus {
                source.clone()
            } else {
                skeletonize(
                    &source,
                    file.lang,
                    extension.eq_ignore_ascii_case("tsx") || extension.eq_ignore_ascii_case("jsx"),
                )
            };
            let mode = if file.is_focus {
                "FULL"
            } else if content == source {
                "UNCHANGED"
            } else {
                "SKELETON"
            };
            results.push(FileResult {
                path: file.relative_path,
                lang: file.lang.to_owned(),
                mode: mode.to_owned(),
                orig_tokens,
                skel_tokens: estimate_tokens(&content),
                content,
            });
        }
        if results.is_empty() && !warnings.is_empty() {
            return Err(warnings.join("\n"));
        }
        let stats = compute_stats(&results);
        let entries: Vec<_> = results
            .iter()
            .map(|file| {
                json!({
                    "path": file.path,
                    "language": file.lang,
                    "mode": file.mode,
                    "original_estimated_tokens": file.orig_tokens,
                    "skeleton_estimated_tokens": file.skel_tokens,
                })
            })
            .collect();
        let mut metadata = json!({
            "stats": stats_json(&stats),
            "files": entries,
            "warnings": warnings,
            "excluded_count": excluded.len(),
            "excluded_files": excluded,
        });
        let markdown = format_markdown(&results, &stats);
        metadata["markdown_estimated_tokens"] = json!(estimate_tokens(&markdown));
        let mut result = CallToolResult::success(vec![ContentBlock::text(markdown)]);
        result.structured_content = Some(metadata);
        Ok(result)
    }
}

#[tool_router]
impl SkelServer {
    #[tool(
        description = "List supported source files under the server root without reading their contents. Focus and exclude globs are relative to the selected path. Focus overrides exclude; excluded paths and matching patterns are reported separately.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn skel_list_files(
        &self,
        Parameters(params): Parameters<ContextParams>,
    ) -> CallToolResult {
        self.execute(params, Operation::List).await
    }

    #[tool(
        description = "Generate Markdown code skeletons for a file or directory under the server root. Focus globs keep matching files in full and override exclude globs. Excluded paths and matching patterns appear in metadata only. Python/TS/JS use AST; other supported languages use lexical pruning. UNCHANGED means no pruning occurred, including on parse failure. Token counts are estimates.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn skel_skeletonize(
        &self,
        Parameters(params): Parameters<ContextParams>,
    ) -> CallToolResult {
        self.execute(params, Operation::Context).await
    }
}

#[tool_handler(
    name = "skel",
    instructions = "Read-only code context tools scoped to the startup directory. Use skel_list_files to inspect paths and skel_skeletonize to get context with compression estimates. Focus and exclude globs are relative to the selected path (basename for a single file); focus overrides exclude. Existing CLI ignore rules apply before these globs; .gitignore is not read. Scan limits include excluded files. UNCHANGED files may contain only declarations or have parse errors."
)]
impl ServerHandler for SkelServer {}

fn tool_error(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

fn stats_json(stats: &RepoStats) -> Value {
    json!({
        "file_count": stats.file_count,
        "original_estimated_tokens": stats.orig_tokens,
        "skeleton_estimated_tokens": stats.skel_tokens,
        "saved_estimated_tokens": stats.saved_tokens,
        "reduction_percent": stats.ratio,
        "token_estimation": "UTF-8 bytes / 3.5; excludes Markdown overhead",
    })
}

/// Run until the MCP client closes its stdio connection. Stdout is protocol only.
pub fn serve(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let server = SkelServer::new(root).map_err(std::io::Error::other)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok(())
    })
}
