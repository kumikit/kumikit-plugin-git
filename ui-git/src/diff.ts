/** Pure classification and formatting helpers for Git status and unified diffs. */

export interface FileChange {
  path: string;
  origPath?: string;
  index: string;
  worktree: string;
  status: string;
}

export interface GitStatus {
  branch?: string | null;
  oid?: string | null;
  upstream?: string | null;
  ahead: number;
  behind: number;
  files: FileChange[];
}

export interface Commit {
  hash: string;
  shortHash: string;
  author: string;
  date: string;
  subject: string;
}

export type DiffLineKind = 'add' | 'del' | 'hunk' | 'meta' | 'ctx';

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
}

/** Classifies each line of a unified diff for styling (never interpreted as HTML). */
export function parseUnifiedDiff(diff: string): DiffLine[] {
  if (!diff) return [];
  return diff.split('\n').map((text) => {
    if (text.startsWith('@@')) return { kind: 'hunk', text };
    if (text.startsWith('+++') || text.startsWith('---') || text.startsWith('diff ') || text.startsWith('index ')) {
      return { kind: 'meta', text };
    }
    if (text.startsWith('+')) return { kind: 'add', text };
    if (text.startsWith('-')) return { kind: 'del', text };
    return { kind: 'ctx', text };
  });
}

/** Short status letter shown beside a changed file. */
export function statusBadge(f: FileChange): string {
  if (f.index === '?' && f.worktree === '?') return '?';
  if (f.worktree !== '.' && f.worktree !== ' ') return f.worktree;
  return f.index === '.' ? 'M' : f.index;
}

/** Formats the branch header line. */
export function formatBranch(st: GitStatus): string {
  const name = st.branch || '(no branch)';
  const parts: string[] = [];
  if (st.ahead > 0) parts.push(`+${st.ahead}`);
  if (st.behind > 0) parts.push(`-${st.behind}`);
  return parts.length > 0 ? `${name} (${parts.join(' / ')})` : name;
}
