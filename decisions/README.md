# decisions/

Interactive review docs for significant changes. Each topic is a
self-contained HTML file you open locally, mark up, and export to a
`decisions.json` the team (or a future agent) can read back.

## Files

- `index.html` — **plate-chart (Charts.css) slide type** review. Open it in
  a browser:

  ```
  open decisions/index.html
  ```

- `decisions.schema.json` — JSON Schema for the exported file.
- `decisions.json` — (created when you export) your verdicts + comments.

## How to use

1. Open `index.html`. Read the **Critical changes** table and the
   **Decision points**.
2. For each decision pick **Approve / Revise / Reject** and add notes.
   Every change autosaves to this browser (localStorage) — safe to close
   and come back.
3. Click **Export decisions.json**:
   - Chrome/Edge: a save dialog lets you write straight into this
     `decisions/` folder (File System Access API).
   - Other browsers: it downloads `decisions.json` — move it here.
4. To resume later (or on another machine), click **Load decisions.json**
   and pick the file.

The page makes **no network requests** — consistent with the project's
local-only, no-CDN ethos.

## Acting on the feedback

`decisions.json` is the source of truth for what you decided. A follow-up
session can read it and action every `verdict: "revise"` / `"reject"` with
your `comment` as the instruction. Decisions tagged *revisitable* (D1, D4)
are the most likely to want changes.
