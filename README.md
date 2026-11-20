# kumikit-plugin-git

First-party Git plugins for [Kumikit](https://github.com/kumikit/kumikit):

- **`kumikit.git`** (`git/`): WASM core plugin (`kumikit:tool/plugin@0.1.0`) offering read-only
  MCP tools (`status`, `diff`, `log`, `show`, `branches`) and workspace RPC methods
  (`status`, `diff`, `log`).
- **`kumikit.ui-git`** (`ui-git/`): Level 2 sandboxed panel (`inspector.tab` "Changes")
  showing changed files, unified diffs and recent commits from `kumikit.git`.

## Build and test

Core plugin:

```sh
cargo test -p kumikit-git
cargo build --target wasm32-wasip2 --release -p kumikit-git
KUMIKIT_DIR=../kumikit cargo test -p kumikit-git-e2e
```

UI panel:

```sh
cd ui-git
npm ci
npm run check
npm test
KUMIKIT_UI_SDK=../../kumikit/web/sdk npm run build
```
