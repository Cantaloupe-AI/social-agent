# Cantalog

Local-only macOS desktop app for end-of-day activity capture. Shows today's git commits and Claude Code sessions in a checkable list, plus a free-text "anything worth remembering?" field. One screen. Saves to SQLite, closes itself.

**Status:** v1 prototype. See [STATUS.md](./STATUS.md).

## Setup

```bash
bun install
bun run tauri dev           # launches the app window against Vite HMR
# or, build a debug binary:
bun run tauri build --debug
open src-tauri/target/debug/bundle/macos/Cantalog.app
```

## Tests

```bash
cd src-tauri && cargo test  # Rust backend (28 tests)
bun run vitest run          # Frontend hooks + helpers (13 tests)
```

## Where data lives

- Config: `~/Library/Application Support/cantalog/config.toml`
- DB: `~/Library/Application Support/cantalog/db.sqlite`

The app is fully local — no network calls beyond reading local files and running `git log` in configured repos.

## Keyboard shortcuts

- `j` / `↓` — next activity
- `k` / `↑` — previous activity
- `x` / `Space` — toggle the selected row
- `⌘↵` — save
- `⌘,` — settings
- `Esc` — close (prompts if thoughts has unsaved text)
