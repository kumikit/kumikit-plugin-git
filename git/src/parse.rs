//! Pure Git argument builders and porcelain output parsers, tested natively.

use serde::Serialize;

/// Parsed result of `git status --porcelain=v2 -z --branch`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// Current branch name (`"(detached)"` when detached, or `None` if unreported).
    pub branch: Option<String>,
    /// Current HEAD commit hash, or `"(initial)"`.
    pub oid: Option<String>,
    /// Upstream tracking branch, if any.
    pub upstream: Option<String>,
    /// Commits ahead of upstream.
    pub ahead: u32,
    /// Commits behind upstream.
    pub behind: u32,
    /// Changed, staged, unmerged or untracked files.
    pub files: Vec<FileChange>,
}

/// One file entry from `git status --porcelain=v2 -z`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Repository-relative path.
    pub path: String,
    /// Previous path for renames/copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    /// Index (staged) status character (`'.'`, `'M'`, `'A'`, `'D'`, `'R'`, `'C'`, `'?'`, `'U'`).
    pub index: String,
    /// Worktree (unstaged) status character.
    pub worktree: String,
    /// Short human-readable status label (`"modified"`, `"added"`, `"deleted"`, `"renamed"`,
    /// `"untracked"`, `"conflict"`, etc.).
    pub status: String,
}

/// One commit from `git log -z`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    /// Full 40-char hash.
    pub hash: String,
    /// Abbreviated hash.
    pub short_hash: String,
    /// Author name.
    pub author: String,
    /// Author date (strict ISO 8601).
    pub date: String,
    /// First line of commit message.
    pub subject: String,
}

/// One local branch from `git branch --format=...`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    /// Branch name.
    pub name: String,
    /// Whether this branch is currently checked out.
    pub current: bool,
    /// Abbreviated commit hash.
    pub short_hash: String,
    /// Upstream branch name, if configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
}

/// Safe base flags prepended to every `git` invocation so repository config cannot run
/// fsmonitor or hooks (`docs/spec/11-security.md`).
pub fn base_args() -> Vec<String> {
    vec![
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        "core.hooksPath=".into(),
        "--no-pager".into(),
    ]
}

/// Safe environment variables for every `git` invocation.
pub fn base_env() -> Vec<(String, String)> {
    vec![
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
    ]
}

/// Validates a user-supplied revision string, refusing leading dashes and NUL bytes.
pub fn validate_rev(rev: &str) -> Result<&str, String> {
    let trimmed = rev.trim();
    if trimmed.is_empty() {
        return Err("rev must not be empty".into());
    }
    if trimmed.starts_with('-') {
        return Err("rev must not start with `-`".into());
    }
    if trimmed.contains('\0') {
        return Err("rev must not contain NUL bytes".into());
    }
    Ok(trimmed)
}

/// Validates a user-supplied repository-relative path, refusing NUL bytes and empty strings.
pub fn validate_path(path: &str) -> Result<&str, String> {
    if path.is_empty() {
        return Err("path must not be empty".into());
    }
    if path.contains('\0') {
        return Err("path must not contain NUL bytes".into());
    }
    Ok(path)
}

/// Builds arguments for `git status --porcelain=v2 -z --branch`.
pub fn status_args() -> Vec<String> {
    let mut args = base_args();
    args.extend([
        "status".into(),
        "--porcelain=v2".into(),
        "-z".into(),
        "--branch".into(),
    ]);
    args
}

/// Builds arguments for `git diff` (with `--no-ext-diff` and `--no-textconv`).
pub fn diff_args(path: Option<&str>, staged: bool) -> Result<Vec<String>, String> {
    let mut args = base_args();
    args.extend([
        "diff".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
    ]);
    if staged {
        args.push("--cached".into());
    }
    if let Some(p) = path {
        let p = validate_path(p)?;
        args.push("--".into());
        args.push(p.to_owned());
    }
    Ok(args)
}

/// Builds arguments for `git log -z`.
pub fn log_args(count: u32, path: Option<&str>) -> Result<Vec<String>, String> {
    let count = count.clamp(1, 100);
    let mut args = base_args();
    args.extend([
        "log".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        "-z".into(),
        "-n".into(),
        count.to_string(),
        "--format=%H%x1f%h%x1f%an%x1f%aI%x1f%s".into(),
    ]);
    if let Some(p) = path {
        let p = validate_path(p)?;
        args.push("--".into());
        args.push(p.to_owned());
    }
    Ok(args)
}

/// Builds arguments for `git show` (with `--no-ext-diff` and `--no-textconv`).
pub fn show_args(rev: &str, path: Option<&str>) -> Result<Vec<String>, String> {
    let rev = validate_rev(rev)?;
    let mut args = base_args();
    args.extend([
        "show".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
    ]);
    match path {
        Some(p) => {
            let p = validate_path(p)?;
            args.push(format!("{rev}:{p}"));
        }
        None => {
            args.push("--stat".into());
            args.push("--patch".into());
            args.push(rev.to_owned());
        }
    }
    Ok(args)
}

/// Builds arguments for `git branch`.
pub fn branches_args() -> Vec<String> {
    let mut args = base_args();
    args.extend([
        "branch".into(),
        "--format=%(HEAD)\t%(refname:short)\t%(objectname:short)\t%(upstream:short)".into(),
    ]);
    args
}

fn status_label(index: char, worktree: char) -> &'static str {
    match (index, worktree) {
        ('?', '?') => "untracked",
        ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => "conflict",
        ('R', _) => "renamed",
        ('C', _) => "copied",
        ('A', _) => "added",
        ('D', _) | (_, 'D') => "deleted",
        ('M', _) | (_, 'M') => "modified",
        ('T', _) | (_, 'T') => "typechange",
        _ => "changed",
    }
}

/// Parses `git status --porcelain=v2 -z --branch` bytes.
pub fn parse_status_v2_z(bytes: &[u8]) -> GitStatus {
    let mut out = GitStatus::default();
    let mut parts = bytes.split(|&b| b == 0);
    while let Some(raw) = parts.next() {
        if raw.is_empty() {
            continue;
        }
        let rec = String::from_utf8_lossy(raw);
        if let Some(rest) = rec.strip_prefix("# ") {
            if let Some(v) = rest.strip_prefix("branch.head ") {
                out.branch = Some(v.to_owned());
            } else if let Some(v) = rest.strip_prefix("branch.oid ") {
                out.oid = Some(v.to_owned());
            } else if let Some(v) = rest.strip_prefix("branch.upstream ") {
                out.upstream = Some(v.to_owned());
            } else if let Some(v) = rest.strip_prefix("branch.ab ") {
                let mut ab = v.split_whitespace();
                if let Some(a) = ab.next().and_then(|s| s.strip_prefix('+')) {
                    out.ahead = a.parse().unwrap_or(0);
                }
                if let Some(b) = ab.next().and_then(|s| s.strip_prefix('-')) {
                    out.behind = b.parse().unwrap_or(0);
                }
            }
        } else if let Some(rest) = rec.strip_prefix("1 ") {
            // `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`
            let mut fields = rest.splitn(8, ' ');
            let xy = fields.next().unwrap_or("..");
            let path = fields.nth(6).unwrap_or_default();
            if !path.is_empty() {
                let idx = xy.chars().next().unwrap_or('.');
                let wt = xy.chars().nth(1).unwrap_or('.');
                out.files.push(FileChange {
                    path: path.to_owned(),
                    orig_path: None,
                    index: idx.to_string(),
                    worktree: wt.to_string(),
                    status: status_label(idx, wt).to_owned(),
                });
            }
        } else if let Some(rest) = rec.strip_prefix("2 ") {
            // `2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\0<origPath>`
            let mut fields = rest.splitn(9, ' ');
            let xy = fields.next().unwrap_or("..");
            let path = fields.nth(7).unwrap_or_default();
            let orig = parts
                .next()
                .map(|b| String::from_utf8_lossy(b).into_owned());
            if !path.is_empty() {
                let idx = xy.chars().next().unwrap_or('.');
                let wt = xy.chars().nth(1).unwrap_or('.');
                out.files.push(FileChange {
                    path: path.to_owned(),
                    orig_path: orig.filter(|s| !s.is_empty()),
                    index: idx.to_string(),
                    worktree: wt.to_string(),
                    status: status_label(idx, wt).to_owned(),
                });
            }
        } else if let Some(rest) = rec.strip_prefix("u ") {
            // `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
            let mut fields = rest.splitn(10, ' ');
            let xy = fields.next().unwrap_or("UU");
            let path = fields.nth(8).unwrap_or_default();
            if !path.is_empty() {
                let idx = xy.chars().next().unwrap_or('U');
                let wt = xy.chars().nth(1).unwrap_or('U');
                out.files.push(FileChange {
                    path: path.to_owned(),
                    orig_path: None,
                    index: idx.to_string(),
                    worktree: wt.to_string(),
                    status: "conflict".to_owned(),
                });
            }
        } else if let Some(path) = rec.strip_prefix("? ") {
            out.files.push(FileChange {
                path: path.to_owned(),
                orig_path: None,
                index: "?".into(),
                worktree: "?".into(),
                status: "untracked".into(),
            });
        }
    }
    out
}

/// Parses `git log -z --format=%H%x1f%h%x1f%an%x1f%aI%x1f%s`.
pub fn parse_log_z(bytes: &[u8]) -> Vec<Commit> {
    let mut out = Vec::new();
    for raw in bytes.split(|&b| b == 0) {
        if raw.is_empty() {
            continue;
        }
        let rec = String::from_utf8_lossy(raw);
        let mut fields = rec.splitn(5, '\x1f');
        let (Some(hash), Some(short_hash), Some(author), Some(date), Some(subject)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        out.push(Commit {
            hash: hash.to_owned(),
            short_hash: short_hash.to_owned(),
            author: author.to_owned(),
            date: date.to_owned(),
            subject: subject.to_owned(),
        });
    }
    out
}

/// Parses `git branch --format=%(HEAD)\t%(refname:short)\t%(objectname:short)\t%(upstream:short)`.
pub fn parse_branches(text: &str) -> Vec<Branch> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.splitn(4, '\t');
        let head = fields.next().unwrap_or_default();
        let name = fields.next().unwrap_or_default();
        let short_hash = fields.next().unwrap_or_default();
        let upstream = fields.next().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        out.push(Branch {
            name: name.to_owned(),
            current: head.trim() == "*",
            short_hash: short_hash.to_owned(),
            upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
        });
    }
    out
}

/// Human-readable status text for MCP tool responses.
pub fn format_status_text(st: &GitStatus) -> String {
    let mut lines = Vec::new();
    if let Some(b) = &st.branch {
        let mut head = format!("On branch {b}");
        if st.ahead > 0 || st.behind > 0 {
            head.push_str(&format!(" (ahead {}, behind {})", st.ahead, st.behind));
        }
        lines.push(head);
    }
    if st.files.is_empty() {
        lines.push("Working tree clean.".into());
    } else {
        for f in &st.files {
            match &f.orig_path {
                Some(orig) => {
                    lines.push(format!("{}{} {} -> {}", f.index, f.worktree, orig, f.path))
                }
                None => lines.push(format!("{}{} {}", f.index, f.worktree, f.path)),
            }
        }
    }
    lines.join("\n")
}

/// Human-readable commit log for MCP tool responses.
pub fn format_log_text(commits: &[Commit]) -> String {
    if commits.is_empty() {
        return "No commits.".into();
    }
    commits
        .iter()
        .map(|c| format!("{} {} ({}, {})", c.short_hash, c.subject, c.author, c.date))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Human-readable branches list for MCP tool responses.
pub fn format_branches_text(branches: &[Branch]) -> String {
    if branches.is_empty() {
        return "No branches.".into();
    }
    branches
        .iter()
        .map(|b| {
            let mark = if b.current { "*" } else { " " };
            match &b.upstream {
                Some(u) => format!("{mark} {} {} [{}]", b.name, b.short_hash, u),
                None => format!("{mark} {} {}", b.name, b.short_hash),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2_status_with_branch_renames_conflicts_and_untracked() {
        let raw = b"# branch.oid 1234567890abcdef\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa bbb src/main.rs\0\
1 A. N... 000000 100644 100644 000 ccc added file.txt\0\
2 R. N... 100644 100644 100644 ddd eee R100 new name.rs\0old name.rs\0\
u UU N... 100644 100644 100644 100644 a b c conflict.txt\0\
? untracked.txt\0";
        let st = parse_status_v2_z(raw);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        assert_eq!(st.ahead, 2);
        assert_eq!(st.behind, 1);
        assert_eq!(st.files.len(), 5);
        assert_eq!(st.files[0].path, "src/main.rs");
        assert_eq!(st.files[0].status, "modified");
        assert_eq!(st.files[1].path, "added file.txt");
        assert_eq!(st.files[1].status, "added");
        assert_eq!(st.files[2].path, "new name.rs");
        assert_eq!(st.files[2].orig_path.as_deref(), Some("old name.rs"));
        assert_eq!(st.files[2].status, "renamed");
        assert_eq!(st.files[3].status, "conflict");
        assert_eq!(st.files[4].path, "untracked.txt");
        assert_eq!(st.files[4].status, "untracked");
    }

    #[test]
    fn parses_log_and_branches() {
        let raw = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x1faaaaaaa\x1fAda\x1f2026-11-20T10:00:00Z\x1fInitial commit\0\
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\x1fbbbbbbb\x1fGrace\x1f2026-11-20T11:00:00Z\x1fAdd diff\0";
        let commits = parse_log_z(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].short_hash, "aaaaaaa");
        assert_eq!(commits[1].subject, "Add diff");

        let branches = parse_branches("*\tmain\taaaaaaa\torigin/main\n \tfeat\tbbbbbbb\t\n");
        assert_eq!(branches.len(), 2);
        assert!(branches[0].current);
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert!(!branches[1].current);
        assert_eq!(branches[1].upstream, None);
    }

    #[test]
    fn rejects_option_injection_in_rev_and_disables_hooks_and_fsmonitor() {
        assert!(validate_rev("--exec=sh").is_err());
        assert!(validate_rev("").is_err());
        assert!(validate_rev("HEAD~1").is_ok());

        let args = diff_args(Some("-weird"), true).unwrap();
        assert!(args.windows(2).any(|w| w == ["-c", "core.fsmonitor=false"]));
        assert!(args.windows(2).any(|w| w == ["-c", "core.hooksPath="]));
        assert!(args.contains(&"--no-ext-diff".to_owned()));
        assert!(args.contains(&"--no-textconv".to_owned()));
        assert!(args.windows(2).any(|w| w == ["--", "-weird"]));
    }
}
