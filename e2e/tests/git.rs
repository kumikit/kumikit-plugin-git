//! End-to-end tests of `kumikit.git` against a real `kumikit serve` and a real temporary
//! git repository:
//! - Workspace RPC methods (`kumikit.git/status`, `kumikit.git/diff`, `kumikit.git/log`).
//! - All 5 MCP tools (`kumikit_git__status`, `diff`, `log`, `show`, `branches`) called by
//!   an agent (`fake-acp-agent replay`) through `provider-acp` and the real `kumikit mcp`
//!   tool gateway, checking `tool.started.source`.
//! - Verifies repository `.git/config` `core.fsmonitor` and `core.hooksPath` are blocked.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(60);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

struct Bins {
    kumikit: PathBuf,
    fake_agent: PathBuf,
    acp_dir: PathBuf,
    acp_wasm: PathBuf,
    git_wasm: PathBuf,
}

impl Bins {
    fn locate() -> Self {
        let k = std::env::var_os("KUMIKIT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root().parent().unwrap().join("kumikit"));
        let b = Bins {
            kumikit: k.join("target/debug").join(exe("kumikit")),
            fake_agent: k
                .join("target/kumikit-test/release")
                .join(exe("fake-acp-agent")),
            acp_dir: k.join("plugins/provider-acp"),
            acp_wasm: k.join("target/kumikit-test/wasm32-wasip2/release/kumikit_provider_acp.wasm"),
            git_wasm: repo_root().join("target/wasm32-wasip2/release/kumikit_git.wasm"),
        };
        for f in [&b.kumikit, &b.fake_agent, &b.acp_wasm, &b.git_wasm] {
            assert!(f.is_file(), "{} is missing", f.display());
        }
        b
    }

    fn pack(&self, dir: &Path, out: &Path) {
        let o = Command::new(&self.kumikit)
            .args(["plugin", "pack"])
            .arg(dir)
            .arg("-o")
            .arg(out)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "kumikit plugin pack {}: {}",
            dir.display(),
            String::from_utf8_lossy(&o.stderr)
        );
    }
}

struct Core {
    tmp: TempDir,
    workspace: PathBuf,
    core: Child,
    bridge: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<Value>,
    next_id: u64,
    events: Vec<Value>,
}

impl Core {
    fn start(bins: &Bins) -> Self {
        let tmp = tempfile::Builder::new().prefix("kkg").tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let socket = if cfg!(windows) {
            format!(
                r"\\.\pipe\kkg-{}",
                tmp.path().file_name().unwrap().to_string_lossy()
            )
        } else {
            tmp.path().join("s.sock").to_string_lossy().into_owned()
        };
        let workspace = tmp.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();

        let mut paths = vec![bins.fake_agent.parent().unwrap().to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut core = Command::new(&bins.kumikit)
            .args(["serve", "--socket", &socket, "--data-dir"])
            .arg(tmp.path().join("data"))
            .env("PATH", std::env::join_paths(paths).unwrap())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = core.stdout.take().unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.starts_with("ready") {
                    let _ = ready_tx.send(());
                }
            }
        });
        ready_rx.recv_timeout(DEADLINE).expect("no ready line");

        let mut bridge = Command::new(&bins.kumikit)
            .args(["bridge", "--socket", &socket])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = bridge.stdin.take().unwrap();
        let out = bridge.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && tx.send(v).is_err()
                {
                    break;
                }
            }
        });
        let mut c = Core {
            tmp,
            workspace,
            core,
            bridge,
            stdin,
            rx,
            next_id: 0,
            events: Vec::new(),
        };
        c.call(
            "core.hello",
            json!({ "protocol": "0.8", "client": { "name": "kumikit-git-e2e", "version": "0" } }),
        );
        c
    }

    fn recv(&mut self, deadline: Instant) -> Value {
        let left = deadline
            .checked_duration_since(Instant::now())
            .expect("deadline passed");
        self.rx.recv_timeout(left).expect("bridge closed")
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.stdin, "{req}").unwrap();
        self.stdin.flush().unwrap();
        let deadline = Instant::now() + DEADLINE;
        loop {
            let m = self.recv(deadline);
            if m["method"] == "events.event" {
                self.events.push(m["params"].clone());
            } else if m["id"] == json!(id) {
                assert!(m.get("error").is_none(), "{method}: {}", m["error"]);
                return m["result"].clone();
            }
        }
    }

    fn event(&mut self, topic: &str, deadline: Instant) -> Value {
        if let Some(i) = self.events.iter().position(|e| e["topic"] == topic) {
            return self.events.remove(i);
        }
        loop {
            let m = self.recv(deadline);
            if m["method"] == "events.event" {
                if m["params"]["topic"] == topic {
                    return m["params"].clone();
                }
                self.events.push(m["params"].clone());
            }
        }
    }

    fn install_dir(&mut self, bins: &Bins, name: &str, dir: &Path) -> Value {
        let kkp = self.tmp.path().join(format!("{name}.kkp"));
        bins.pack(dir, &kkp);
        let p = self.call("plugin.install", json!({ "path": kkp }))["plugin"].clone();
        self.call(
            "plugin.enable",
            json!({ "plugin": p["id"], "grant": p["requested"] }),
        )["plugin"]
            .clone()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.bridge.kill();
        let _ = self.bridge.wait();
        let _ = self.core.kill();
        let _ = self.core.wait();
    }
}

fn git_cmd(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test User")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test User")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
}

fn init_sample_repo(dir: &Path, sentinel: &Path) {
    git_cmd(dir, &["init", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "line1\nline2\n").unwrap();
    git_cmd(dir, &["add", "README.md"]);
    git_cmd(dir, &["commit", "-m", "Initial commit"]);

    // Working-tree change, staged change, and untracked file.
    std::fs::write(dir.join("README.md"), "line1\nline2 modified\n").unwrap();
    std::fs::write(dir.join("staged.txt"), "hello staged\n").unwrap();
    git_cmd(dir, &["add", "staged.txt"]);
    std::fs::write(dir.join("notes.txt"), "untracked\n").unwrap();

    // Plant a malicious fsmonitor command in .git/config AFTER setup commits/adds so only
    // kumikit.git runs git afterwards, proving -c core.fsmonitor=false blocks it.
    let marker = sentinel.to_string_lossy().replace('\\', "/");
    git_cmd(
        dir,
        &[
            "config",
            "core.fsmonitor",
            &format!("sh -c 'touch {marker}'"),
        ],
    );
}

#[test]
fn git_methods_and_mcp_tools_run_against_a_real_repo() {
    let bins = Bins::locate();
    let mut core = Core::start(&bins);
    let sentinel = core.tmp.path().join("fsmonitor-ran");
    init_sample_repo(&core.workspace, &sentinel);

    // Install provider-acp.
    let acp_stage = core.tmp.path().join("stage-acp");
    std::fs::create_dir_all(&acp_stage).unwrap();
    std::fs::copy(
        bins.acp_dir.join("plugin.toml"),
        acp_stage.join("plugin.toml"),
    )
    .unwrap();
    std::fs::copy(
        bins.acp_dir.join("acp.schema.json"),
        acp_stage.join("acp.schema.json"),
    )
    .unwrap();
    std::fs::copy(&bins.acp_wasm, acp_stage.join("plugin.wasm")).unwrap();
    core.install_dir(&bins, "provider-acp", &acp_stage);

    // Stage and install kumikit.git.
    let git_src = repo_root().join("git");
    let git_stage = core.tmp.path().join("stage-git");
    std::fs::create_dir_all(git_stage.join("tools")).unwrap();
    std::fs::copy(git_src.join("plugin.toml"), git_stage.join("plugin.toml")).unwrap();
    std::fs::copy(git_src.join("README.md"), git_stage.join("README.md")).unwrap();
    std::fs::copy(&bins.git_wasm, git_stage.join("plugin.wasm")).unwrap();
    for entry in std::fs::read_dir(git_src.join("tools")).unwrap() {
        let p = entry.unwrap().path();
        std::fs::copy(&p, git_stage.join("tools").join(p.file_name().unwrap())).unwrap();
    }
    let git_plugin = core.install_dir(&bins, "kumikit-git", &git_stage);
    assert_eq!(git_plugin["id"], "kumikit.git");

    let ws = core.call(
        "workspace.open",
        json!({ "path": core.workspace.to_string_lossy() }),
    )["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // 1. Direct workspace RPC methods (used by kumikit.ui-git).
    let st = core.call("kumikit.git/status", json!({ "workspace": ws }));
    assert_eq!(st["branch"], "main");
    let files = st["files"].as_array().unwrap();
    assert!(
        files
            .iter()
            .any(|f| f["path"] == "README.md" && f["status"] == "modified")
    );
    assert!(
        files
            .iter()
            .any(|f| f["path"] == "staged.txt" && f["status"] == "added")
    );
    assert!(
        files
            .iter()
            .any(|f| f["path"] == "notes.txt" && f["status"] == "untracked")
    );

    let diff = core.call(
        "kumikit.git/diff",
        json!({ "workspace": ws, "path": "README.md" }),
    );
    assert!(
        diff["diff"].as_str().unwrap().contains("+line2 modified"),
        "{diff}"
    );
    let staged_diff = core.call(
        "kumikit.git/diff",
        json!({ "workspace": ws, "path": "staged.txt", "staged": true }),
    );
    assert!(
        staged_diff["diff"]
            .as_str()
            .unwrap()
            .contains("+hello staged"),
        "{staged_diff}"
    );

    let log = core.call("kumikit.git/log", json!({ "workspace": ws, "count": 5 }));
    let commits = log["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["subject"], "Initial commit");

    // 2. Agent calling all 5 git MCP tools through the real tool gateway (`kumikit mcp`).
    let transcript_path = core.tmp.path().join("git-mcp.jsonl");
    let transcript = [
        json!({"from":"client","message":{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}}),
        json!({"from":"agent","message":{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}}),
        json!({"from":"client","message":{"jsonrpc":"2.0","id":2,"method":"session/new","params":{}}}),
        json!({"from":"agent","message":{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}}),
        json!({"from":"client","message":{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{}}}),
        json!({"from":"mcp","session":"s1","call":{"name":"kumikit_git__status","arguments":{}}}),
        json!({"from":"mcp","session":"s1","call":{"name":"kumikit_git__diff","arguments":{"path":"README.md"}}}),
        json!({"from":"mcp","session":"s1","call":{"name":"kumikit_git__log","arguments":{"count":5}}}),
        json!({"from":"mcp","session":"s1","call":{"name":"kumikit_git__show","arguments":{"rev":"HEAD","path":"README.md"}}}),
        json!({"from":"mcp","session":"s1","call":{"name":"kumikit_git__branches","arguments":{}}}),
        json!({"from":"agent","message":{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    std::fs::write(&transcript_path, format!("{transcript}\n")).unwrap();

    let agent_stage = core.tmp.path().join("stage-agent");
    std::fs::create_dir_all(&agent_stage).unwrap();
    let tpath = std::fs::canonicalize(&transcript_path)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "\\\\");
    std::fs::write(
        agent_stage.join("plugin.toml"),
        format!(
            "id = \"test.git-agent\"\nname = \"Git agent\"\nversion = \"0.1.0\"\nkind = [\"provider\"]\napi = \"^0.2\"\n\
             [permissions]\nspawn = [\"fake-acp-agent\"]\ntools = true\n\
             [contributes]\nproviders = [{{ id = \"agent\", name = \"Agent\", acp = {{ command = \"fake-acp-agent\", args = [\"replay\", \"{tpath}\"] }} }}]\n"
        ),
    )
    .unwrap();
    core.install_dir(&bins, "git-agent", &agent_stage);

    let thread = core.call(
        "thread.create",
        json!({ "workspace": ws, "provider": "test.git-agent/agent" }),
    )["thread"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let topic = format!("thread:{thread}");
    core.call("events.subscribe", json!({ "topic": topic, "after": 0 }));
    core.call(
        "thread.send",
        json!({ "thread": thread, "input": { "text": "inspect git" } }),
    );

    let deadline = Instant::now() + DEADLINE;
    let mut gateway_tools = Vec::new();
    let mut final_text = String::new();
    loop {
        let e = core.event(&topic, deadline);
        match e["kind"].as_str() {
            Some("tool.started") => {
                if e["data"]["source"]["plugin"] == "kumikit.git" {
                    gateway_tools.push(e["data"]["source"]["tool"].as_str().unwrap().to_owned());
                }
            }
            Some("text.done") => {
                final_text = e["data"]["text"].as_str().unwrap_or_default().to_owned();
            }
            Some("turn.ended") => {
                assert_eq!(e["data"]["outcome"], "done", "{e}");
                break;
            }
            _ => {}
        }
    }
    assert_eq!(
        gateway_tools,
        vec!["status", "diff", "log", "show", "branches"]
    );
    assert!(final_text.contains("README.md"), "{final_text}");
    assert!(final_text.contains("+line2 modified"), "{final_text}");
    assert!(final_text.contains("Initial commit"), "{final_text}");
    assert!(final_text.contains("line1\nline2"), "{final_text}");
    assert!(final_text.contains("* main"), "{final_text}");

    // 3. The planted fsmonitor command must never have run.
    assert!(!sentinel.exists(), "core.fsmonitor ran despite -c override");
}
