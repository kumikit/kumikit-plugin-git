import { describe, expect, it } from 'vitest';

import { formatBranch, parseUnifiedDiff, statusBadge } from '../src/diff';

describe('ui-git diff and status helpers', () => {
  it('classifies unified diff lines safely as plain text', () => {
    const diff = [
      'diff --git a/a.txt b/a.txt',
      '--- a/a.txt',
      '+++ b/a.txt',
      '@@ -1,2 +1,2 @@',
      ' keep',
      '-<script>alert(1)</script>',
      '+safe line',
    ].join('\n');
    const lines = parseUnifiedDiff(diff);
    expect(lines.map((l) => l.kind)).toEqual(['meta', 'meta', 'meta', 'hunk', 'ctx', 'del', 'add']);
    expect(lines[5].text).toBe('-<script>alert(1)</script>');
  });

  it('formats branch ahead/behind and status badges', () => {
    expect(formatBranch({ branch: 'main', ahead: 2, behind: 1, files: [] })).toBe('main (+2 / -1)');
    expect(formatBranch({ branch: 'main', ahead: 0, behind: 0, files: [] })).toBe('main');
    expect(statusBadge({ path: 'x', index: '?', worktree: '?', status: 'untracked' })).toBe('?');
    expect(statusBadge({ path: 'x', index: '.', worktree: 'M', status: 'modified' })).toBe('M');
    expect(statusBadge({ path: 'x', index: 'A', worktree: '.', status: 'added' })).toBe('A');
  });
});
