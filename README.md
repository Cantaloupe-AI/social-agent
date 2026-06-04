# Happycampr Carousels

Local-only macOS desktop app for end-of-day activity capture. Shows today's git commits and Claude Code sessions in a checkable list, plus a free-text "anything worth remembering?" field. One screen. Saves to SQLite, closes itself.

**Status:** v1 prototype. See [STATUS.md](./STATUS.md).

## Setup

```bash
bun install
bun run tauri dev           # launches the app window against Vite HMR
# or, build a debug binary:
bun run tauri build --debug
open src-tauri/target/debug/bundle/macos/Happycampr Carousels.app
```

## Tests

```bash
cd src-tauri && cargo test  # Rust backend (78 tests)
bun run vitest run          # Frontend hooks + helpers (25 tests)
```

## CLI (`happycampr-carousels-cli`)

A scriptable surface over the same SQLite/bun pipeline the app uses — handy
for testing the generation loop and for other agents. Dev invocation:

```bash
cd src-tauri
cargo run -q --bin happycampr-carousels-cli -- <command>
# or build it once: cargo build --release --bin happycampr-carousels-cli
#   → src-tauri/target/release/happycampr-carousels-cli

happycampr-carousels-cli carousel create --label "Launch week" [--orientation vertical|landscape]
happycampr-carousels-cli carousel list
happycampr-carousels-cli slide add <carousel-id> --file slide.md  # or --stdin / --content "…"  [--title T]
happycampr-carousels-cli import --new --file deck.md              # splits on `# Slide …` blocks
happycampr-carousels-cli import --carousel <id> --file deck.md    # append to an existing carousel
happycampr-carousels-cli generate <carousel-id>                   # blocks, streams logs, opens the PDF
happycampr-carousels-cli status <carousel-id> [--log 40] [--watch]
happycampr-carousels-cli open <carousel-id>                       # open the generated PDF
```

`generate` runs headless: it supervises the bun driver, prints its output
live, and opens the finished PDF (`--no-open` to skip). It reads/writes the
same `db.sqlite` as the app, so a carousel made here shows up in the UI and
vice-versa. `generate` is always blocking and supervised; Ctrl-C cancels the
run; a hard kill before it finalizes may leave the carousel `generating`
until the next `generate` (which resets run state). For async use, background
the command at the shell. Only `import` deck files you wrote/reviewed (their
text is fed to the design agent).

## Where data lives

- Config: `~/Library/Application Support/happycampr-carousels/config.toml`
- DB: `~/Library/Application Support/happycampr-carousels/db.sqlite`

The app is fully local — no network calls beyond reading local files and running `git log` in configured repos.

## Keyboard shortcuts

- `j` / `↓` — next activity
- `k` / `↑` — previous activity
- `x` / `Space` — toggle the selected row
- `⌘↵` — save
- `⌘,` — settings
- `Esc` — close (prompts if thoughts has unsaved text)
