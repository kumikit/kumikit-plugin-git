# Git (`kumikit.git`)

Read-only Git tools (`status`, `diff`, `log`, `show`, `branches`) exposed through
Kumikit's MCP tool gateway, plus workspace RPC methods (`status`, `diff`, `log`) for the
`kumikit.ui-git` Changes panel.

## Security

Every `git` invocation runs with `-c core.fsmonitor=false -c core.hooksPath= --no-pager`,
`--no-ext-diff --no-textconv` (where applicable), `GIT_TERMINAL_PROMPT=0`, and
`GIT_OPTIONAL_LOCKS=0`, so repository configuration cannot launch hooks, fsmonitor, or
external diff/textconv helpers.
