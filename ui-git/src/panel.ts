// Sandboxed Changes panel (`kumikit.ui-git`): reads `kumikit.git/status`, `diff`, `log`
// through the panel bridge, renders text safely via textContent, and refreshes on mount,
// on Refresh button click, and on `thread.updated` workspace events (never on a timer).
import { connectPanel, PanelError } from '@kumikit/ui-sdk/panel';

import { formatBranch, parseUnifiedDiff, statusBadge, type Commit, type FileChange, type GitStatus } from './diff';

const branchEl = document.getElementById('branch')!;
const refreshBtn = document.getElementById('refresh') as HTMLButtonElement;
const filesEl = document.getElementById('files')!;
const diffTitleEl = document.getElementById('diff-title')!;
const diffEl = document.getElementById('diff')!;
const commitsEl = document.getElementById('commits')!;

function clear(el: Element): void {
  while (el.firstChild) el.removeChild(el.firstChild);
}

function emptyItem(el: Element, text: string, cls = 'empty'): void {
  clear(el);
  const node = document.createElement(el.tagName === 'UL' ? 'li' : 'p');
  node.className = cls;
  node.textContent = text;
  el.appendChild(node);
}

const ctx = await connectPanel();

let selectedPath: string | null = null;
let currentFiles: FileChange[] = [];
let loading = false;

async function showDiff(file: FileChange | null): Promise<void> {
  if (!file) {
    diffTitleEl.textContent = 'Diff';
    emptyItem(diffEl, 'Select a changed file to inspect its diff.');
    return;
  }
  diffTitleEl.textContent = `Diff — ${file.path}`;
  if (file.status === 'untracked') {
    emptyItem(diffEl, 'Untracked file (no git diff yet).');
    return;
  }
  try {
    // Prefer unstaged diff when worktree changed; fall back to staged diff.
    const preferStaged = file.worktree === '.' && file.index !== '.';
    let res = (await ctx.connection.call('kumikit.git/diff', {
      path: file.path,
      staged: preferStaged,
    })) as { diff: string };
    if (!res.diff && !preferStaged && file.index !== '.') {
      res = (await ctx.connection.call('kumikit.git/diff', {
        path: file.path,
        staged: true,
      })) as { diff: string };
    }
    const lines = parseUnifiedDiff(res.diff);
    if (lines.length === 0) {
      emptyItem(diffEl, 'No textual diff.');
      return;
    }
    clear(diffEl);
    for (const line of lines) {
      const p = document.createElement('p');
      p.className = `diff-line diff-${line.kind}`;
      p.textContent = line.text || ' ';
      diffEl.appendChild(p);
    }
  } catch (e) {
    const msg = e instanceof PanelError ? e.message : String(e);
    emptyItem(diffEl, msg, 'error');
  }
}

function renderFiles(files: FileChange[]): void {
  currentFiles = files;
  if (files.length === 0) {
    emptyItem(filesEl, 'Working tree clean.');
    selectedPath = null;
    void showDiff(null);
    return;
  }
  if (!selectedPath || !files.some((f) => f.path === selectedPath)) {
    selectedPath = files[0].path;
  }
  clear(filesEl);
  for (const f of files) {
    const li = document.createElement('li');
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'file-btn';
    btn.setAttribute('aria-selected', String(f.path === selectedPath));

    const badge = document.createElement('span');
    badge.className = `badge badge-${f.status}`;
    badge.textContent = statusBadge(f);

    const pathSpan = document.createElement('span');
    pathSpan.className = 'path';
    pathSpan.textContent = f.origPath ? `${f.origPath} -> ${f.path}` : f.path;

    btn.append(badge, pathSpan);
    btn.addEventListener('click', () => {
      selectedPath = f.path;
      renderFiles(currentFiles);
    });
    li.appendChild(btn);
    filesEl.appendChild(li);
  }
  const active = files.find((f) => f.path === selectedPath) ?? null;
  void showDiff(active);
}

function renderCommits(commits: Commit[]): void {
  if (commits.length === 0) {
    emptyItem(commitsEl, 'No commits yet.');
    return;
  }
  clear(commitsEl);
  for (const c of commits) {
    const li = document.createElement('li');
    li.className = 'commit-row';

    const hash = document.createElement('span');
    hash.className = 'commit-hash';
    hash.textContent = c.shortHash;

    const subj = document.createElement('span');
    subj.className = 'commit-subject';
    subj.textContent = c.subject;

    const author = document.createElement('span');
    author.className = 'commit-author';
    author.textContent = c.author;

    li.append(hash, subj, author);
    commitsEl.appendChild(li);
  }
}

async function refresh(): Promise<void> {
  if (loading) return;
  loading = true;
  refreshBtn.disabled = true;
  try {
    const [status, log] = (await Promise.all([
      ctx.connection.call('kumikit.git/status', {}),
      ctx.connection.call('kumikit.git/log', { count: 20 }),
    ])) as [GitStatus, { commits: Commit[] }];
    branchEl.textContent = formatBranch(status);
    renderFiles(status.files);
    renderCommits(log.commits);
    document.body.dataset.ready = '1';
  } catch (e) {
    const msg = e instanceof PanelError ? e.message : String(e);
    branchEl.textContent = 'Unavailable';
    emptyItem(filesEl, msg, 'error');
    emptyItem(diffEl, '');
    emptyItem(commitsEl, '');
    document.body.dataset.ready = '1';
  } finally {
    loading = false;
    refreshBtn.disabled = false;
  }
}

refreshBtn.addEventListener('click', () => void refresh());

if (ctx.address?.workspace) {
  void ctx.connection.subscribe(`workspace:${ctx.address.workspace}`, 0, (ev) => {
    if (ev.kind === 'thread.updated') void refresh();
  });
}

await refresh();
