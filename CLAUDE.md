<!--
  Agent context file. Read by Claude Code and other agents entering this repo.
  Humans browsing the repo should read README.md first, then STATUS.md.
  XML tags are used because this file is parsed, not rendered.
-->

<project>
  <name>Cantalog</name>
  <what>Local-only macOS desktop app for end-of-day activity capture and LinkedIn carousel authoring.</what>
  <one_liner>
    Today's git commits + Claude Code sessions in a checkable list, a free-text
    "anything worth remembering?" field, and a Slides tab for building
    Cantaloupe-branded LinkedIn carousels. Saves to SQLite, closes itself.
  </one_liner>
  <status>v0.1 prototype. See STATUS.md (dated 2026-04-19) for the authoritative snapshot.</status>
</project>

<intent>
  <primary>
    Log insights throughout the day and translate them into LinkedIn carousels
    that follow Cantaloupe's brand guidelines. The carousel authoring spec lives
    in design/carousel-manifest.md and is the source of truth for any generated
    carousel.
  </primary>
  <secondary>
    Capture quick thoughts throughout the day. The main view's "Anything worth
    remembering?" field and the Slides tab both serve this — low-friction,
    keyboard-driven, local.
  </secondary>
  <non_goals>
    Not a sync product. Not a team product. Not networked (no API calls beyond
    reading local files and running `git log` in configured repos).
  </non_goals>
</intent>

<architecture>
  <frontend>
    React 19 + TypeScript, Tailwind v4 (CSS-first, no tailwind.config.js —
    configured via @theme inline in src/styles/globals.css), shadcn/radix
    primitives at src/components/ui/.
  </frontend>
  <desktop>Tauri v2. Vite HMR in dev; WKWebView in shipped builds.</desktop>
  <backend>
    Rust crate `cantalog_lib` (src-tauri/). SQLite for persistence, chrono for
    dates, serde for config + IPC types. Five Tauri commands in commands.rs are
    thin wrappers over the module functions.
  </backend>
  <ipc_boundary>
    src/lib/tauri.ts holds typed `invoke()` wrappers. src/lib/types.ts mirrors
    the Rust types in src-tauri/src/types.rs. Keep them in sync by hand.
  </ipc_boundary>
</architecture>

<data_layout>
  <config>~/Library/Application Support/cantalog/config.toml</config>
  <db>~/Library/Application Support/cantalog/db.sqlite</db>
  <tables>
    <table name="entries">Per-day save: date (PK), thoughts, selected activity ids.</table>
    <table name="carousels">Carousel metadata: id (PK), label, created_at, updated_at.</table>
    <table name="slides">
      Slide rows: id (PK), carousel_id (FK), order_index, content, timestamps.
      Content is free-form text today; the manifest defines the YAML-structured
      form a future renderer will consume.
    </table>
  </tables>
  <schema_source>src-tauri/src/db.rs (migrate() is the only schema source; idempotent).</schema_source>
</data_layout>

<file_map>
  <backend root="src-tauri/src/">
    <file path="lib.rs">Module tree + Tauri builder + plugin + handler registration.</file>
    <file path="main.rs">Delegates to cantalog_lib::run.</file>
    <file path="commands.rs">Tauri commands (thin wrappers); generate_carousel_pdf parks the child + watcher, delegates the core to generation.rs.</file>
    <file path="generation.rs">Tauri-free generation core shared by the Tauri command and cantalog-cli: repo_root, next_run_dir, spawn_pipe_tee, prepare_run, spawn_driver, finalize. The ONE place run-dir naming + the crash-finalize safety net live.</file>
    <file path="cli.rs">cantalog-cli handlers (pure, Result-returning): parse_deck, cmd_carousel_*, cmd_slide_add, cmd_import, cmd_status, cmd_open, cmd_generate. Injected stdin/opener for tests.</file>
    <file path="bin/cantalog-cli.rs">Thin clap parser over cli.rs. `cargo run --bin cantalog-cli -- …`.</file>
    <file path="config.rs">TOML load/save + default_config.</file>
    <file path="db.rs">open / migrate / save_entry / load_entry + carousel+slide CRUD.</file>
    <file path="types.rs">Activity, Entry, Config, ActivitySource, GitConfig, ClaudeCodeConfig.</file>
    <file path="sources/git.rs">parse_git_log + fetch_today.</file>
    <file path="sources/claude_code.rs">Claude Code JSONL scanner + first-user-message extraction.</file>
  </backend>
  <frontend root="src/">
    <file path="App.tsx">Shell: hooks + components + keyboard + save flow.</file>
    <file path="components/">ActivityList, ActivityRow, ConfigDialog, SaveButton, ThoughtsField, CarouselsView, CarouselEditor, ui/*.</file>
    <file path="hooks/">useActivities, useKeyboard, useTodayEntry.</file>
    <file path="lib/">tauri.ts (invoke wrappers), types.ts (TS mirrors), merge.ts, time.ts, utils.ts.</file>
  </frontend>
  <design root="design/">
    <file path="design-tokens.json">DTCG-format tokens (color, typography, spacing, layout). Canonical source.</file>
    <file path="carousel-manifest.md">Human-readable carousel authoring spec (canvas, grid, shapes, safe zones).</file>
    <file path="carousel.manifest.json">Machine-readable numeric mirror of the manifest, for renderers.</file>
    <file path="cantaloupe.design-contracts.json">Validation rules (safe zones, line-height floors, shape/color limits).</file>
    <file path="carousel_example.md">Five-slide example exercising every template.</file>
  </design>
</file_map>

<test_philosophy>
  <principles>
    <p>Test-first. Every phase committed only after its checkpoints went green.</p>
    <p>No `#[ignore]` in Rust. No `any` in TypeScript. No `.unwrap()` in production Rust (tests only).</p>
    <p>No mocks where a parameter will do. Inject time/paths; don't monkey-patch globals.</p>
    <p>Fixtures live next to the code they exercise: src-tauri/tests/fixtures/{git,claude}/.</p>
  </principles>
  <counts as_of="2026-05-18">
    <rust>78 tests (cargo test) across config.rs, db.rs, generation.rs (8), cli.rs (17), sources/git.rs, sources/claude_code.rs.</rust>
    <typescript>25 tests (bun run vitest run) across hooks/ and lib/.</typescript>
    <total>103.</total>
  </counts>
  <commands>
    <cmd purpose="rust">cd src-tauri &amp;&amp; cargo test</cmd>
    <cmd purpose="rust_check">cd src-tauri &amp;&amp; cargo check --all-targets</cmd>
    <cmd purpose="ts">bun run vitest run</cmd>
    <cmd purpose="ts_check">bun run tsc --noEmit</cmd>
    <cmd purpose="bundle">bun run vite build</cmd>
    <cmd purpose="debug_app">bun run tauri build --debug</cmd>
  </commands>
  <generation_model>
    For any TEST or CHECK carousel generation (verifying a rebrand, the
    pipeline, a contract change — anything that is not a real deliverable),
    use `claude-sonnet-4-6` for BOTH agents (implementation AND manager). The
    per-carousel default is opus×2 (`claude-opus-4-7`), which is correct for
    real output but too slow/expensive for test runs (Opus thinking blocks +
    SDK rate-limit backoff push a 5-slide check past 30 min). Set impl_model
    and manager_model to sonnet on the carousel before `cantalog-cli generate`
    for these runs. Opus×2 only for genuine deliverables.
  </generation_model>
  <ui_testing_gap>
    Playwright is not installed and not on the approved deps list. UI smoke
    tests are manual — see the checklist at the bottom of STATUS.md. Do not add
    Playwright without explicit approval.
  </ui_testing_gap>
</test_philosophy>

<roadmap>
  <done>
    <item>End-of-day activity capture (git commits + Claude Code sessions).</item>
    <item>Settings dialog, keyboard-driven flow, save-and-close behavior.</item>
    <item>Slides tab with carousel + slide CRUD (commit 1177c69).</item>
    <item>Design tokens, carousel manifest, validation contracts, example doc landed in design/.</item>
  </done>
  <next priority_order="true">
    <item>YAML-structured slide schema: upgrade the slides table's content blob to the manifest's front-matter format.</item>
    <item>Live preview: render a slide at 1080×1350 in the editor. No PDF yet.</item>
    <item>Contract-driven validation at save time (reject slides that violate cantaloupe.design-contracts.json).</item>
    <item>PDF export: concat per-slide pages into a single {slug}-v{N}.pdf.</item>
    <item>Insight → carousel pipeline: a "promote this thought to a slide" path that lifts a day's thought field into a new carousel.</item>
  </next>
  <explicitly_future>
    Sync, collaboration, scheduled posting, LinkedIn API integration. None of
    these are in scope for the local-only v0.x line.
  </explicitly_future>
</roadmap>

<companion_docs>
  <doc path="README.md">Setup, keyboard shortcuts, data paths. Start here for humans.</doc>
  <doc path="STATUS.md">
    Authoritative snapshot of build state, test counts, decisions, deviations,
    and known limitations. Re-read when your assumptions diverge from code.
  </doc>
  <doc path="algorithmic-guide.md">
    LinkedIn algorithm reference (Q1 2026). Use when reasoning about what to
    carousel-ify, topic coherence, or content strategy. Do not treat it as
    marketing copy — it's a load-bearing product-decision input.
  </doc>
  <doc path="design/carousel-manifest.md">
    The authoring spec for carousels. If you are generating, editing, or
    validating slide content, this is the source of truth.
  </doc>
  <doc path="design/cantaloupe.design-contracts.json">
    The hard validation rules. A renderer MUST fail the build on violation.
  </doc>
  <doc path="roadmap.md">Currently empty. STATUS.md's "Known limitations" + this file's `&lt;roadmap&gt;` are the active list.</doc>
</companion_docs>

<conventions>
  <commits>
    Conventional-commit style: `feat(scope): …`, `fix(scope): …`, `chore(scope): …`.
    Recent history (`git log --oneline`) is the reference — match its shape.
    Never amend a published commit. Never force-push.
  </commits>
  <dependencies>
    Keep the dep list tight. STATUS.md documents everything added and why. Before
    adding a new dep, check whether a stdlib or existing dep already does the job
    (STATUS.md has specific examples: tempfile, filetime, mockall — all declined).
  </dependencies>
  <types>
    When changing src-tauri/src/types.rs, also update src/lib/types.ts. The
    mirror is maintained by hand.
  </types>
  <css>
    Tailwind v4 with @theme inline. No tailwind.config.js. Design tokens are
    the source of truth — don't introduce one-off color or spacing values.
  </css>
  <errors>
    No silent failures. Anywhere. Every fallible operation either:
      (a) propagates the error to the caller (Result, throw, reject), OR
      (b) logs to a visible surface (console.error, eprintln!, run.log),
          ideally both.
    Forbidden patterns:
      - `.catch(() =&gt; {})` / `catch {}` / empty catch blocks
      - `let _ = fallible_op()` in production code (tests are OK)
      - Returning early from a worker thread without flipping the
        observable state it owns (see `generate_carousel_pdf` watcher).
    When a long-running process supervises a child, the supervisor MUST
    finalize observable state on every exit path — including mutex
    poisoning, race conditions, and "this can't happen" cases. Use a
    Drop guard or single finalize() function reachable from every
    branch; do not duplicate the cleanup at every return.
  </errors>
</conventions>

<gotchas>
  <item>
    `unescape_project_name` in src-tauri/src/sources/claude_code.rs is lossy by
    spec — dashes inside real directory names get mangled when Claude project
    paths are decoded. Chip labels may look odd. Don't "fix" without re-reading
    the TODO(spec) note in STATUS.md.
  </item>
  <item>
    The "Loading today's activity…" flash is unavoidable without a cache. It is
    cosmetic, not a bug.
  </item>
  <item>
    Chip width is capped at 18ch. Long project paths truncate visually; hover
    shows full text via `title`.
  </item>
  <item>
    Escape-with-unsaved-text uses `window.confirm()`. Native but unstyled —
    intentional for v1.
  </item>
  <item>
    On macOS, the spec requires the app to terminate after save (not just close
    the window). `quit_app` Tauri command handles this — don't replace it with
    `getCurrentWindow().close()` without understanding why.
  </item>
</gotchas>

<!-- hippo:start -->
## Project Memory (Hippo)

Before starting work, load relevant context:
```bash
hippo context --auto --budget 1500
```

When you learn something important:
```bash
hippo remember "<lesson>"
```

When you hit an error or discover a gotcha:
```bash
hippo remember "<what went wrong and why>" --error
```

After significant discussions or decisions, capture context:
```bash
hippo capture --stdin <<< 'summary of what was decided'
```

After completing work successfully:
```bash
hippo outcome --good
```
<!-- hippo:end -->
