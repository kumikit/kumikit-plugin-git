//! `kumikit.git`: read-only Git tools (via Kumikit's MCP tool gateway) and workspace RPC
//! methods (`status`, `diff`, `log`) for the `kumikit.ui-git` Changes panel.
//!
//! Built solely with the public `kumikit-tool` SDK (`kumikit:tool/plugin@0.1.0`). Every
//! `git` invocation disables `core.fsmonitor`, `core.hooksPath`, external diff drivers,
//! textconv filters, interactive prompts, and optional locks.

pub mod parse;

use kumikit_tool::{
    CallContext, Error, ErrorCode, Plugin, SpawnRequest, Tool, ToolCall, ToolResult, process,
};
use serde_json::{Value, json};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

struct Git;

fn err(code: ErrorCode, msg: impl Into<String>) -> Error {
    Error::new(code, msg)
}

fn run_git(args: Vec<String>) -> Result<Vec<u8>, Error> {
    let child = process::spawn(&SpawnRequest {
        program: "git".into(),
        args,
        cwd: None,
        env: parse::base_env(),
        secret_env: Vec::new(),
    })?;
    child.close_stdin();
    let mut out = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = child.read_stdout(64 * 1024)? {
        if out.len() + chunk.len() > MAX_OUTPUT_BYTES {
            let left = MAX_OUTPUT_BYTES.saturating_sub(out.len());
            out.extend_from_slice(&chunk[..left]);
            truncated = true;
        } else {
            out.extend_from_slice(&chunk);
        }
    }
    let status = child.wait()?;
    if status.code != Some(0) {
        let stderr = child
            .read_stderr(16 * 1024)
            .ok()
            .flatten()
            .unwrap_or_default();
        let msg = String::from_utf8_lossy(&stderr).trim().to_owned();
        let fallback = format!("git exited with status {:?}", status.code);
        return Err(err(
            ErrorCode::Io,
            if msg.is_empty() { fallback } else { msg },
        ));
    }
    if truncated {
        out.extend_from_slice(b"\n[output truncated at 1 MiB]\n");
    }
    Ok(out)
}

fn do_status() -> Result<parse::GitStatus, Error> {
    let bytes = run_git(parse::status_args())?;
    Ok(parse::parse_status_v2_z(&bytes))
}

fn do_diff(path: Option<&str>, staged: bool) -> Result<String, Error> {
    let args = parse::diff_args(path, staged).map_err(|e| err(ErrorCode::InvalidArgument, e))?;
    let bytes = run_git(args)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn do_log(count: u32, path: Option<&str>) -> Result<Vec<parse::Commit>, Error> {
    let args = parse::log_args(count, path).map_err(|e| err(ErrorCode::InvalidArgument, e))?;
    match run_git(args) {
        Ok(bytes) => Ok(parse::parse_log_z(&bytes)),
        // Empty repository with no commits yet: return an empty list instead of failing.
        Err(e) if e.message.contains("does not have any commits yet") => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

fn do_show(rev: &str, path: Option<&str>) -> Result<String, Error> {
    let args = parse::show_args(rev, path).map_err(|e| err(ErrorCode::InvalidArgument, e))?;
    let bytes = run_git(args)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn do_branches() -> Result<Vec<parse::Branch>, Error> {
    let bytes = run_git(parse::branches_args())?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse::parse_branches(&text))
}

impl Plugin for Git {
    fn activate() -> Result<(), Error> {
        Ok(())
    }

    fn deactivate() {}

    fn handle(method: String, params: String, _ctx: CallContext) -> Result<String, Error> {
        let input: Value = serde_json::from_str(&params).unwrap_or_else(|_| json!({}));
        match method.as_str() {
            "status" => {
                let st = do_status()?;
                serde_json::to_string(&st).map_err(|e| err(ErrorCode::Internal, e.to_string()))
            }
            "diff" => {
                let path = input.get("path").and_then(Value::as_str);
                let staged = input
                    .get("staged")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let diff = do_diff(path, staged)?;
                Ok(json!({ "diff": diff }).to_string())
            }
            "log" => {
                let count = input
                    .get("count")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(20);
                let path = input.get("path").and_then(Value::as_str);
                let commits = do_log(count, path)?;
                Ok(json!({ "commits": commits }).to_string())
            }
            other => Err(err(ErrorCode::NotFound, format!("unknown method: {other}"))),
        }
    }
}

impl Tool for Git {
    fn call(call: ToolCall) -> Result<ToolResult, Error> {
        let input: Value = serde_json::from_str(&call.input).unwrap_or_else(|_| json!({}));
        match call.name.as_str() {
            "status" => match do_status() {
                Ok(st) => Ok(ToolResult {
                    text: parse::format_status_text(&st),
                    structured: serde_json::to_string(&st).ok(),
                    is_error: false,
                }),
                Err(e) => Ok(ToolResult::failed(e.message)),
            },
            "diff" => {
                let path = input.get("path").and_then(Value::as_str);
                let staged = input
                    .get("staged")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                match do_diff(path, staged) {
                    Ok(diff) if diff.is_empty() => Ok(ToolResult::text("No differences.")),
                    Ok(diff) => Ok(ToolResult::text(diff)),
                    Err(e) => Ok(ToolResult::failed(e.message)),
                }
            }
            "log" => {
                let count = input
                    .get("count")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(20);
                let path = input.get("path").and_then(Value::as_str);
                match do_log(count, path) {
                    Ok(commits) => Ok(ToolResult {
                        text: parse::format_log_text(&commits),
                        structured: Some(json!({ "commits": commits }).to_string()),
                        is_error: false,
                    }),
                    Err(e) => Ok(ToolResult::failed(e.message)),
                }
            }
            "show" => {
                let Some(rev) = input.get("rev").and_then(Value::as_str) else {
                    return Ok(ToolResult::failed("rev is required"));
                };
                let path = input.get("path").and_then(Value::as_str);
                match do_show(rev, path) {
                    Ok(text) => Ok(ToolResult::text(text)),
                    Err(e) => Ok(ToolResult::failed(e.message)),
                }
            }
            "branches" => match do_branches() {
                Ok(branches) => Ok(ToolResult {
                    text: parse::format_branches_text(&branches),
                    structured: Some(json!({ "branches": branches }).to_string()),
                    is_error: false,
                }),
                Err(e) => Ok(ToolResult::failed(e.message)),
            },
            other => Err(err(ErrorCode::NotFound, format!("unknown tool: {other}"))),
        }
    }
}

kumikit_tool::export_tool!(Git);
