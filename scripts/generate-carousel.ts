#!/usr/bin/env bun
/**
 * Carousel generation driver.
 *
 * Spawned by the Tauri backend as: `bun run scripts/generate-carousel.ts
 * <carousel_id> <run_dir>`. The backend picks the run dir up front and
 * persists it to `carousels.run_dir` *before* spawning, so we trust the
 * path we're given. Stdout/stderr are captured by the parent and tee'd
 * to `<run_dir>/run.log` — so `log()` here only writes to stderr (the
 * parent owns the file).
 *
 * Per slide: up to MAX_ITERS rounds of (implementation agent → puppeteer
 * render → manager agent review). Loop exits early on accept; on
 * exhaustion the slide is marked failed. After all slides accept, all
 * latest-version PDFs are concatenated into the carousel.pdf.
 */

import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { PDFDocument } from "pdf-lib";
import { ORIENTATION_DIMS, renderSlide, type Orientation } from "./render-slide.ts";

// ─── Constants ──────────────────────────────────────────────────────────────

const REPO_ROOT = resolve(import.meta.dir, "..");
const DATA_DIR = join(homedir(), "Library", "Application Support", "happycampr-carousels");
const DB_PATH = join(DATA_DIR, "db.sqlite");

const IMPL_PROMPT_PATH = join(REPO_ROOT, "prompts", "implementation_agent.md");
const MGR_PROMPT_PATH = join(REPO_ROOT, "prompts", "manager_agent.md");

/**
 * Brand logo lockups + the per-slide filenames the agent embeds. Both
 * variants are copied into each slide's working dir at run start so the
 * generated HTML can reference one by a simple relative path
 * (`<img src="happycampr-logo.svg">`). Keeps the slide folder
 * self-contained — puppeteer's `file://` resolver picks it up without
 * any absolute paths leaking into the HTML.
 *
 * Two variants ship because the wordmark must stay legible on whatever
 * surface the slide header sits on. The agent picks per slide background:
 *   - `happycampr-logo.svg`         — burnt lockup, for the default
 *                                     marshmallow (light) surface.
 *   - `happycampr-logo-inverse.svg` — marshmallow lockup, for the
 *                                     burnt (dark) surface-inverse slide
 *                                     (e.g. takeaway).
 * The selection rule lives in design-tokens.json#branding,
 * happycampr.design-contracts.json#branding.headerLogo, and the agent
 * prompts. See carousel.manifest.json#branding.themingFutureWork for the
 * future multi-brand plan.
 */
const HEADER_LOGOS = [
  {
    src: join(REPO_ROOT, "design", "assets", "happycampr-logo-full.svg"),
    filename: "happycampr-logo.svg",
  },
  {
    src: join(
      REPO_ROOT,
      "design",
      "assets",
      "happycampr-logo-full-marshmallow.svg",
    ),
    filename: "happycampr-logo-inverse.svg",
  },
];

/**
 * Design files inlined into every agent's system prompt at run start.
 * Order matters only for human-readability of the resulting block; the
 * SDK doesn't care.
 */
const DESIGN_SPEC_FILES = [
  "design/design-tokens.json",
  "design/carousel-manifest.md",
  "design/carousel.manifest.json",
  "design/happycampr.design-contracts.json",
  "design/carousel_example.md",
];

/**
 * Fallback model id when a carousel row's `impl_model` / `manager_model`
 * is NULL. The DB intentionally stores NULL rather than this literal,
 * so changing the default is a one-line edit here — no migration.
 */
const DEFAULT_MODEL = "claude-opus-4-7";

/** Maximum implement→review rounds per slide before we give up. */
const MAX_ITERS = 4;

/**
 * Idle watchdog for SDK streams. If the agent's async iterator yields no
 * message for this long, we assume the upstream SSE has silently stalled
 * (observed once: manager review hung mid-Read on slide 6, no result, no
 * error, dead air for hours). Aborts the query so the iter-retry loop
 * can decide whether to retry or fail the slide.
 *
 * Set well above the worst-case legit thinking gap. Longest happy-path
 * silence seen in run logs is ~2 min between rate_limit_event and the
 * next thinking block; 4 min gives ~2× headroom.
 */
const SDK_IDLE_TIMEOUT_MS = 240_000;

/**
 * Idle window used while the stream is in the *throttled* regime — see the
 * `throttled` flag in `driveAgent`. A `rate_limit_event` enters that regime;
 * the SDK then emits sparse internal retry `system` messages every 1–4 min
 * (no forward progress) until the 429 clears. Observed on kitchen-sink
 * run-6 slide 1: 19 such messages over ~33 min, then a >240 s gap that
 * killed a still-alive slide because each retry `system` re-armed the
 * short guard. Keying the long window off the throttled *state* (not just
 * the rate_limit_event message) lets those retries ride out; the state
 * clears on the first real forward-progress message, restoring the tight
 * dead-air guard. 15 min still backstops a genuinely dead throttled stream.
 */
const SDK_RATE_LIMIT_IDLE_TIMEOUT_MS = 900_000;

// ─── DB helpers ─────────────────────────────────────────────────────────────

interface CarouselRow {
  id: string;
  label: string;
  slug: string | null;
  status: string;
  /** NULL = use DEFAULT_MODEL. Same on manager_model. */
  impl_model: string | null;
  manager_model: string | null;
  /**
   * Canvas orientation. The DB column is NOT NULL with DEFAULT
   * 'vertical'; we still defensively coerce here so a hand-edited DB
   * with a stray value doesn't blow up the run.
   */
  orientation: string;
}

interface SlideRow {
  id: string;
  carousel_id: string;
  order_index: number;
  content: string;
  status: string;
}

function openDb(): Database {
  if (!existsSync(DB_PATH)) {
    throw new Error(`Database not found at ${DB_PATH}. Open the app once.`);
  }
  const db = new Database(DB_PATH);
  db.exec("PRAGMA foreign_keys = ON;");
  return db;
}

function loadCarousel(db: Database, id: string): CarouselRow {
  const row = db
    .query<CarouselRow, [string]>(
      "SELECT id, label, slug, status, impl_model, manager_model, orientation FROM carousels WHERE id = ?",
    )
    .get(id);
  if (!row) throw new Error(`Carousel ${id} not found`);
  return row;
}

function resolveOrientation(raw: string): Orientation {
  if (raw === "vertical" || raw === "landscape") return raw;
  // CHECK constraint should make this unreachable; log and fall back so
  // a misbehaving migration doesn't kill the whole run.
  process.stderr.write(
    `[bun-driver] WARN: unknown orientation "${raw}", defaulting to vertical\n`,
  );
  return "vertical";
}

function loadSlides(db: Database, carouselId: string): SlideRow[] {
  return db
    .query<SlideRow, [string]>(
      "SELECT id, carousel_id, order_index, content, status FROM slides WHERE carousel_id = ? ORDER BY order_index ASC",
    )
    .all(carouselId);
}

function setCarouselRunFinished(
  db: Database,
  id: string,
  status: "done" | "failed",
  pdfPath: string | null,
) {
  const now = new Date().toISOString();
  db.run(
    `UPDATE carousels
     SET status = ?, run_finished_at = ?, pdf_path = ?, updated_at = ?
     WHERE id = ?`,
    [status, now, pdfPath, now, id],
  );
}

interface LatestPdf {
  slideId: string;
  pdfPath: string;
}

function loadLatestSlidePdfs(
  db: Database,
  slideIds: string[],
): LatestPdf[] {
  const out: LatestPdf[] = [];
  for (const slideId of slideIds) {
    const row = db
      .query<{ pdf_path: string | null }, [string]>(
        `SELECT pdf_path FROM slide_versions
         WHERE slide_id = ?
         ORDER BY version_number DESC LIMIT 1`,
      )
      .get(slideId);
    if (!row || !row.pdf_path) {
      throw new Error(`Slide ${slideId} has no rendered PDF`);
    }
    out.push({ slideId, pdfPath: row.pdf_path });
  }
  return out;
}

async function concatPdfs(inputs: string[], outputPath: string): Promise<void> {
  const merged = await PDFDocument.create();
  for (const input of inputs) {
    const bytes = await readFile(input);
    const src = await PDFDocument.load(bytes);
    const pages = await merged.copyPages(src, src.getPageIndices());
    for (const page of pages) merged.addPage(page);
  }
  const out = await merged.save();
  await writeFile(outputPath, out);
}

function setSlideStatus(
  db: Database,
  id: string,
  status: string,
  lastError: string | null = null,
) {
  const now = new Date().toISOString();
  db.run(
    `UPDATE slides SET status = ?, last_error = ?, updated_at = ? WHERE id = ?`,
    [status, lastError, now, id],
  );
}

function insertSlideVersion(
  db: Database,
  slideId: string,
  htmlPath: string,
): { id: string; version_number: number } {
  const id = randomUUID();
  const now = new Date().toISOString();
  const next = db
    .query<{ next: number }, [string]>(
      "SELECT COALESCE(MAX(version_number) + 1, 1) AS next FROM slide_versions WHERE slide_id = ?",
    )
    .get(slideId);
  const versionNumber = next?.next ?? 1;
  db.run(
    `INSERT INTO slide_versions
       (id, slide_id, version_number, html_path, created_at)
     VALUES (?, ?, ?, ?, ?)`,
    [id, slideId, versionNumber, htmlPath, now],
  );
  return { id, version_number: versionNumber };
}

function updateSlideVersionRenders(
  db: Database,
  id: string,
  screenshotPath: string,
  pdfPath: string,
) {
  db.run(
    `UPDATE slide_versions SET screenshot_path = ?, pdf_path = ? WHERE id = ?`,
    [screenshotPath, pdfPath, id],
  );
}

interface AcceptedVersion {
  version_number: number;
  html_path: string;
  pdf_path: string | null;
}

/**
 * Find the highest-numbered slide_version for `slideId` that the manager
 * accepted. Returns null if there's never been an accept.
 *
 * The latest manager_feedback row per version is taken to be the verdict
 * (one feedback per version per run today, but the LIMIT 1 ORDER BY
 * created_at DESC is defensive for future re-reviews).
 */
function findLatestAcceptedVersion(
  db: Database,
  slideId: string,
): AcceptedVersion | null {
  const row = db
    .query<AcceptedVersion, [string]>(
      `SELECT v.version_number, v.html_path, v.pdf_path
         FROM slide_versions v
         JOIN slide_feedback f ON f.slide_version_id = v.id
        WHERE v.slide_id = ?
          AND f.id = (
            SELECT id FROM slide_feedback
              WHERE slide_version_id = v.id
              ORDER BY created_at DESC
              LIMIT 1
          )
          AND f.accepted = TRUE
        ORDER BY v.version_number DESC
        LIMIT 1`,
    )
    .get(slideId);
  return row ?? null;
}

function insertSlideFeedback(
  db: Database,
  slideVersionId: string,
  accepted: boolean,
  feedback: string,
) {
  const id = randomUUID();
  const now = new Date().toISOString();
  db.run(
    `INSERT INTO slide_feedback
       (id, slide_version_id, accepted, feedback, created_at)
     VALUES (?, ?, ?, ?, ?)`,
    [id, slideVersionId, accepted ? 1 : 0, feedback, now],
  );
}

// ─── Design spec inlining ──────────────────────────────────────────────────

/**
 * Read every file in DESIGN_SPEC_FILES once and concatenate them into a
 * single block suitable for embedding in an agent's system prompt. The
 * Claude Code binary applies ephemeral cache_control to the system
 * prompt by default — so this big block only costs full input tokens
 * on the first agent call per run; every subsequent call reads it from
 * cache at ~10% the price.
 */
async function loadDesignSpec(): Promise<string> {
  const sections = await Promise.all(
    DESIGN_SPEC_FILES.map(async (rel) => {
      const text = await readFile(join(REPO_ROOT, rel), "utf8");
      return `## ${rel}\n\n${text}`;
    }),
  );
  const sep = "==========================================================";
  return [
    sep,
    "EMBEDDED DESIGN SPEC (already in your context — do NOT use the",
    "Read tool to fetch any of these files; refer to them by section",
    "heading below).",
    sep,
    "",
    sections.join("\n\n---\n\n"),
    "",
    sep,
    "END EMBEDDED DESIGN SPEC",
    sep,
  ].join("\n");
}

// ─── Implementation agent invocation ───────────────────────────────────────

async function driveAgent(opts: {
  cwd: string;
  systemPrompt: string;
  prompt: string;
  allowedTools: string[];
  label: string;
  /** Model id passed through to the SDK. Caller resolves NULL → DEFAULT_MODEL. */
  model: string;
}): Promise<void> {
  const abortController = new AbortController();
  const q = query({
    prompt: opts.prompt,
    options: {
      model: opts.model,
      cwd: opts.cwd,
      systemPrompt: opts.systemPrompt,
      permissionMode: "auto",
      allowedTools: opts.allowedTools,
      abortController,
    },
  });

  let resultMessage:
    | {
        type: "result";
        subtype: string;
        is_error: boolean;
        usage?: {
          input_tokens?: number;
          cache_read_input_tokens?: number;
          cache_creation_input_tokens?: number;
        };
      }
    | null = null;
  let messageCount = 0;

  // Idle watchdog: reset on every SDK event. If it fires, the upstream
  // stream has gone silent — abort the query and surface a clear error.
  let watchdogTripped = false;
  let idleTimer: ReturnType<typeof setTimeout> | null = null;
  const armWatchdog = (ms: number) => {
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      watchdogTripped = true;
      process.stderr.write(
        `  [sdk:${opts.label}] idle watchdog: no message in ${ms}ms — aborting\n`,
      );
      abortController.abort();
    }, ms);
  };
  const disarmWatchdog = () => {
    if (idleTimer) {
      clearTimeout(idleTimer);
      idleTimer = null;
    }
  };

  // Log every SDK message so a stuck agent is visible in run.log instead of
  // silent. Per CLAUDE.md §conventions/errors: no silent failures.
  // We don't dump full content (that would flood the log on long thinking
  // turns); just type + a short discriminator so we can see progress.
  // Throttled regime: a `rate_limit_event` means the SDK is now backing
  // off a 429 and will emit only sparse internal retry `system` messages
  // (no forward progress) until it clears. While throttled we arm the
  // long window on *every* message so those retries don't re-arm the tight
  // guard and kill a still-alive slide (run-6 slide 1: 19 retry `system`s
  // over ~33 min, then a >240 s gap tripped the short timer). The first
  // real forward-progress message (assistant/user/result) means the 429
  // cleared — drop back to the tight dead-air guard. `system` messages
  // leave the regime unchanged (init burst at start is harmless: not yet
  // throttled, and it arrives sub-second).
  let throttled = false;
  try {
    armWatchdog(SDK_IDLE_TIMEOUT_MS);
    for await (const msg of q) {
      messageCount++;
      const msgType = (msg as { type?: string }).type;
      process.stderr.write(
        `  [sdk:${opts.label}] #${messageCount} ${describeSdkMessage(msg)}\n`,
      );
      if (msgType === "result") {
        resultMessage = msg as typeof resultMessage;
      }
      if (msgType === "rate_limit_event") {
        throttled = true;
      } else if (
        msgType === "assistant" ||
        msgType === "user" ||
        msgType === "result"
      ) {
        throttled = false;
      }
      armWatchdog(
        throttled ? SDK_RATE_LIMIT_IDLE_TIMEOUT_MS : SDK_IDLE_TIMEOUT_MS,
      );
    }
  } catch (err) {
    if (watchdogTripped) {
      throw new Error(
        `${opts.label} idle watchdog tripped after ${messageCount} message(s) — SDK stream went silent for ${SDK_IDLE_TIMEOUT_MS}ms`,
      );
    }
    throw err;
  } finally {
    disarmWatchdog();
  }
  if (watchdogTripped) {
    throw new Error(
      `${opts.label} idle watchdog tripped after ${messageCount} message(s) — SDK stream went silent for ${SDK_IDLE_TIMEOUT_MS}ms`,
    );
  }
  process.stderr.write(
    `  [sdk:${opts.label}] stream ended after ${messageCount} message(s)\n`,
  );
  if (!resultMessage) {
    throw new Error(
      `${opts.label} ended without a result message (got ${messageCount} message(s) total)`,
    );
  }
  // Log usage so we can verify prompt caching is engaging. Big
  // cache_creation on the first call, big cache_read on every call after.
  // If cache_read stays 0 across calls, caching isn't working — investigate.
  const u = resultMessage.usage;
  if (u) {
    process.stderr.write(
      `  [sdk:${opts.label}] usage input=${u.input_tokens ?? 0}` +
        ` cache_create=${u.cache_creation_input_tokens ?? 0}` +
        ` cache_read=${u.cache_read_input_tokens ?? 0}\n`,
    );
  }
  if (resultMessage.is_error) {
    throw new Error(
      `${opts.label} errored: ${resultMessage.subtype}`,
    );
  }
}

/** Compact, log-safe summary of an SDK message for run.log. */
function describeSdkMessage(msg: unknown): string {
  if (typeof msg !== "object" || msg === null) return "<non-object>";
  const m = msg as Record<string, unknown>;
  const type = String(m.type ?? "<no-type>");
  // For result messages, surface the subtype so success/error is visible.
  if (type === "result") {
    return `result subtype=${String(m.subtype ?? "?")} is_error=${
      m.is_error ?? false
    }`;
  }
  // For assistant/user messages, surface the content shape.
  if (type === "assistant" || type === "user") {
    const content = (m.message as { content?: unknown })?.content;
    if (Array.isArray(content)) {
      const blocks = content.map((b) => {
        if (b && typeof b === "object" && "type" in b) {
          const bt = String((b as { type: unknown }).type);
          if (bt === "tool_use") {
            return `tool_use(${(b as { name?: string }).name ?? "?"})`;
          }
          if (bt === "tool_result") {
            return "tool_result";
          }
          return bt;
        }
        return "?";
      });
      return `${type} blocks=[${blocks.join(",")}]`;
    }
    return type;
  }
  return type;
}

async function runImplementer(opts: {
  slideDir: string;
  versionFilename: string;
  systemPrompt: string;
  iterationFeedback: string | null;
  prevHtmlFilename: string | null;
  model: string;
}): Promise<void> {
  const promptParts = [
    `Working directory: ${opts.slideDir}`,
    `Read source.md.`,
    `Write your output to: ${opts.versionFilename}`,
  ];
  if (opts.prevHtmlFilename) {
    promptParts.push(
      `Previous version is ${opts.prevHtmlFilename}. Use it as your starting point.`,
    );
  }
  if (opts.iterationFeedback) {
    promptParts.push(
      `feedback.md exists with this manager review. Address every point:\n${opts.iterationFeedback}`,
    );
  } else {
    promptParts.push(`This is the first iteration. No feedback yet.`);
  }
  await driveAgent({
    cwd: opts.slideDir,
    systemPrompt: opts.systemPrompt,
    prompt: promptParts.join("\n\n"),
    allowedTools: ["Read", "Write", "Edit"],
    label: "Implementation agent",
    model: opts.model,
  });
}

interface ManagerVerdict {
  accepted: boolean;
  feedback: string;
}

/**
 * Parse the manager's review.json. Tolerant only of *wrapping* the model
 * sometimes adds around otherwise-valid JSON (UTF-8 BOM, ```json fences,
 * stray prose) — it narrows to the outer `{ … }` and parses that. It does
 * NOT guess at malformed content: if the JSON itself is invalid (e.g.
 * unescaped `"` inside `feedback`, which Sonnet has produced), it returns
 * an error so the caller can retrigger the write rather than act on a
 * mangled or hallucinated verdict.
 */
function parseVerdict(
  raw: string,
): { ok: true; verdict: ManagerVerdict } | { ok: false; error: string } {
  let t = raw.replace(/^﻿/, "").trim();
  const fence = t.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fence) t = fence[1].trim();
  const first = t.indexOf("{");
  const last = t.lastIndexOf("}");
  if (first >= 0 && last > first) t = t.slice(first, last + 1);

  let parsed: unknown;
  try {
    parsed = JSON.parse(t);
  } catch (e) {
    return { ok: false, error: `invalid JSON: ${(e as Error).message}` };
  }
  if (
    typeof parsed !== "object" ||
    parsed === null ||
    typeof (parsed as { accepted?: unknown }).accepted !== "boolean" ||
    typeof (parsed as { feedback?: unknown }).feedback !== "string"
  ) {
    return {
      ok: false,
      error: 'missing required fields { accepted: bool, feedback: string }',
    };
  }
  return { ok: true, verdict: parsed as ManagerVerdict };
}

/**
 * Max times we re-ask the manager to (re)write review.json within a single
 * review. A malformed or missing verdict retriggers the write with a
 * corrective hint — it does NOT fail the slide on the first bad file. Only
 * after this many genuinely unusable attempts do we surface an error to
 * the per-slide loop.
 */
const MANAGER_REVIEW_ATTEMPTS = 3;

async function runManager(opts: {
  slideDir: string;
  versionFilename: string; // v{N}.html
  screenshotFilename: string; // v{N}.png
  systemPrompt: string;
  model: string;
}): Promise<ManagerVerdict> {
  const reviewPath = join(opts.slideDir, "review.json");
  let lastError = "";

  for (let attempt = 1; attempt <= MANAGER_REVIEW_ATTEMPTS; attempt++) {
    await rm(reviewPath, { force: true });

    const promptParts = [
      `Working directory: ${opts.slideDir}`,
      `Review ${opts.versionFilename} (rendered to ${opts.screenshotFilename}).`,
      `Write your verdict to review.json with shape { "accepted": bool, "feedback": string }.`,
    ];
    if (attempt > 1) {
      promptParts.push(
        `RETRY (${attempt}/${MANAGER_REVIEW_ATTEMPTS}): your previous review.json was unusable — ${lastError}. ` +
          `Write review.json again as a SINGLE valid JSON object and nothing else: ` +
          `no markdown code fences, no prose before or after. Inside the "feedback" ` +
          `string, every double-quote must be escaped as \\" and every newline as \\n ` +
          `(do not paste raw quotes from the spec or the screenshot). Keep feedback concise.`,
      );
    }

    try {
      await driveAgent({
        cwd: opts.slideDir,
        systemPrompt: opts.systemPrompt,
        prompt: promptParts.join("\n\n"),
        allowedTools: ["Read", "Write"],
        label: "Manager agent",
        model: opts.model,
      });
    } catch (e) {
      // The SDK stream stalled / was aborted by the idle watchdog, or some
      // other transient agent failure. Per the watchdog's own contract
      // ("the iter-retry loop can decide whether to retry or fail") this
      // retriggers the whole review instead of failing the slide and
      // moving on — same policy as a malformed verdict below.
      lastError = `manager stream failed (${(e as Error).message})`;
      process.stderr.write(
        `  [manager] attempt ${attempt}/${MANAGER_REVIEW_ATTEMPTS}: ${lastError} — retriggering\n`,
      );
      continue;
    }

    if (!existsSync(reviewPath)) {
      lastError = "no review.json was written";
    } else {
      const result = parseVerdict(await readFile(reviewPath, "utf8"));
      if (result.ok) {
        if (attempt > 1) {
          process.stderr.write(
            `  [manager] valid review.json on attempt ${attempt}/${MANAGER_REVIEW_ATTEMPTS}\n`,
          );
        }
        return result.verdict;
      }
      lastError = result.error;
    }
    process.stderr.write(
      `  [manager] attempt ${attempt}/${MANAGER_REVIEW_ATTEMPTS}: ${lastError} — retriggering write\n`,
    );
  }

  throw new Error(
    `Manager agent failed to produce valid review.json after ${MANAGER_REVIEW_ATTEMPTS} attempts: ${lastError}`,
  );
}

// ─── Per-slide pipeline (Phase 3: impl + render, no manager) ───────────────

async function generateSlide(opts: {
  db: Database;
  slide: SlideRow;
  slideIndex: number;
  runDir: string;
  implPrompt: string;
  managerPrompt: string;
  implModel: string;
  managerModel: string;
  orientation: Orientation;
  force: boolean;
  log: (line: string) => void;
}) {
  const {
    db,
    slide,
    slideIndex,
    runDir,
    implPrompt,
    managerPrompt,
    implModel,
    managerModel,
    orientation,
    force,
    log,
  } = opts;
  const slideFolderName = `${String(slideIndex).padStart(2, "0")}-${slide.id}`;
  const slideDir = join(runDir, "slides", slideFolderName);
  await mkdir(slideDir, { recursive: true });

  // Freeze the source markdown at run start.
  await writeFile(join(slideDir, "source.md"), slide.content, "utf8");

  // Copy both brand logo variants into the slide dir so the agent's HTML
  // can reference one with `<img src="happycampr-logo.svg">` (or the
  // inverse) and puppeteer resolves it relative to the file:// URL of the
  // rendered HTML. copyFile (rather than symlink) is intentional: the
  // slide dir is committed to disk as the run's artifact, and a symlink
  // that points back into the repo would break if the repo moves.
  for (const logo of HEADER_LOGOS) {
    await copyFile(logo.src, join(slideDir, logo.filename));
  }

  // Skip the agent loop if we have a previously-accepted version whose
  // markdown matches the current slide content. The PDF concat step picks
  // up the prior run's pdf via slide_versions.pdf_path (latest version
  // wins), so we don't need to insert a new version row — the existing
  // accepted one IS the current one.
  //
  // `force` (HAPPYCAMPR_FORCE=1 / `generate --force`) disables this. The
  // skip is keyed on slide markdown only; it cannot see that the design
  // spec, agent prompts, or brand assets changed. After any spec/asset
  // edit, force a re-render so accepted slides pick up the new rules.
  const accepted = force ? null : findLatestAcceptedVersion(db, slide.id);
  if (force) {
    log("  ⟳ force: re-rendering even though markdown is unchanged");
  }
  if (accepted) {
    const priorSourcePath = join(dirname(accepted.html_path), "source.md");
    const priorPdfMissing =
      !accepted.pdf_path || !existsSync(accepted.pdf_path);
    const priorSourceMissing = !existsSync(priorSourcePath);
    if (priorSourceMissing || priorPdfMissing) {
      log(
        `  prior accepted v${accepted.version_number} files missing` +
          ` (source.md=${!priorSourceMissing}, pdf=${!priorPdfMissing});` +
          ` regenerating`,
      );
    } else {
      const priorSource = (
        await readFile(priorSourcePath, "utf8")
      ).trim();
      const currentSource = slide.content.trim();
      if (priorSource === currentSource) {
        log(
          `  ✓ skipping — markdown unchanged since accepted v${accepted.version_number}`,
        );
        setSlideStatus(db, slide.id, "accepted");
        return;
      }
      log(
        `  markdown changed since accepted v${accepted.version_number}; regenerating`,
      );
    }
  }

  let lastFeedback: string | null = null;
  let prevHtmlFilename: string | null = null;

  for (let iter = 1; iter <= MAX_ITERS; iter++) {
    const htmlFilename = `v${iter}.html`;
    const pngFilename = `v${iter}.png`;
    const pdfFilename = `v${iter}.pdf`;
    const htmlPath = join(slideDir, htmlFilename);
    const pngPath = join(slideDir, pngFilename);
    const pdfPath = join(slideDir, pdfFilename);

    // Step 1 — implementation agent writes v{iter}.html
    setSlideStatus(db, slide.id, "generating");
    if (lastFeedback != null) {
      await writeFile(join(slideDir, "feedback.md"), lastFeedback, "utf8");
    }
    log(`  iter ${iter}: implementation agent`);
    await runImplementer({
      slideDir,
      versionFilename: htmlFilename,
      systemPrompt: implPrompt,
      iterationFeedback: lastFeedback,
      prevHtmlFilename: prevHtmlFilename,
      model: implModel,
    });
    if (!existsSync(htmlPath)) {
      throw new Error(`Implementation agent did not write ${htmlFilename}`);
    }
    const version = insertSlideVersion(db, slide.id, htmlPath);

    // Step 2 — render with puppeteer at the carousel's active orientation.
    log(`  iter ${iter}: render`);
    await renderSlide({ htmlPath, pngPath, pdfPath, orientation });
    updateSlideVersionRenders(db, version.id, pngPath, pdfPath);

    // Step 3 — manager agent reviews
    setSlideStatus(db, slide.id, "reviewing");
    log(`  iter ${iter}: manager review`);
    const verdict = await runManager({
      slideDir,
      versionFilename: htmlFilename,
      screenshotFilename: pngFilename,
      systemPrompt: managerPrompt,
      model: managerModel,
    });
    insertSlideFeedback(db, version.id, verdict.accepted, verdict.feedback);

    if (verdict.accepted) {
      log(`  iter ${iter}: ✓ accepted`);
      setSlideStatus(db, slide.id, "accepted");
      return;
    }

    log(`  iter ${iter}: ✗ rejected — ${verdict.feedback.split("\n")[0]}`);
    lastFeedback = verdict.feedback;
    prevHtmlFilename = htmlFilename;
  }

  setSlideStatus(
    db,
    slide.id,
    "failed",
    `manager rejected ${MAX_ITERS} iterations`,
  );
  throw new Error(
    `Slide ${slide.id} not accepted after ${MAX_ITERS} iterations`,
  );
}

// ─── Main ──────────────────────────────────────────────────────────────────

async function main() {
  const carouselId = process.argv[2];
  const runDir = process.argv[3];
  if (!carouselId || !runDir) {
    console.error(
      "Usage: bun run scripts/generate-carousel.ts <carousel_id> <run_dir>",
    );
    process.exit(1);
  }
  if (!existsSync(runDir)) {
    throw new Error(`run_dir does not exist: ${runDir}`);
  }

  const db = openDb();
  const carousel = loadCarousel(db, carouselId);
  if (!carousel.slug) {
    throw new Error(`Carousel ${carouselId} has no slug — reopen the app once`);
  }

  const slides = loadSlides(db, carouselId);
  if (slides.length === 0) {
    throw new Error("Carousel has no slides");
  }

  // The Rust supervisor captures stdout/stderr and writes to run.log;
  // here we only emit to stderr.
  const log = (line: string) => {
    process.stderr.write(`${line}\n`);
  };

  // Resolve per-carousel model overrides. NULL falls back to DEFAULT_MODEL —
  // the literal default lives only here (not in the DB) so changing it is
  // a one-line edit.
  const implModel = carousel.impl_model || DEFAULT_MODEL;
  const managerModel = carousel.manager_model || DEFAULT_MODEL;
  const orientation = resolveOrientation(carousel.orientation);
  const dims = ORIENTATION_DIMS[orientation];

  // Force re-render of every slide, bypassing the markdown-unchanged skip.
  // Set when the spec/prompts/assets changed and accepted slides are stale.
  const force = process.env.HAPPYCAMPR_FORCE === "1";

  log(`Starting run for carousel "${carousel.label}" (${slides.length} slide(s)).`);
  log(`Run dir: ${runDir}`);
  log(`Models: impl=${implModel}, manager=${managerModel}`);
  log(`Orientation: ${orientation} (${dims.widthPx} × ${dims.heightPx})`);
  if (force) log("Force mode: re-rendering all slides (skip disabled)");

  // Read the design spec ONCE up front and embed it into both agents'
  // system prompts. This avoids the per-iteration Read-tool round trips
  // that were burning input-tokens-per-minute and tripping rate limits
  // on the very first slide. Files stay on disk; the agent just sees
  // them as context instead of having to fetch them.
  //
  // The ACTIVE ORIENTATION header sits *above* the design spec so the
  // agent reads it first and knows which `orientations.X` and
  // `canvas.X` branches to follow.
  const designSpec = await loadDesignSpec();
  log(`Embedded design spec: ${designSpec.length.toLocaleString()} chars`);
  const orientationHeader = [
    "==========================================================",
    `ACTIVE ORIENTATION: ${orientation} (${dims.widthPx} × ${dims.heightPx})`,
    `Use the \`${orientation}\` branch of every per-orientation rule below`,
    `(orientations.${orientation} in carousel.manifest.json,`,
    ` canvas.${orientation} in happycampr.design-contracts.json,`,
    ` layout.canvas.${orientation} in design-tokens.json,`,
    ` and the maxColumnPx.${orientation} variants for body / code).`,
    `Ignore the other branch.`,
    "==========================================================",
    "",
  ].join("\n");
  const implRolePrompt = await Bun.file(IMPL_PROMPT_PATH).text();
  const managerRolePrompt = await Bun.file(MGR_PROMPT_PATH).text();
  const implPrompt = `${implRolePrompt}\n\n${orientationHeader}\n${designSpec}`;
  const managerPrompt = `${managerRolePrompt}\n\n${orientationHeader}\n${designSpec}`;

  let allOk = true;
  for (let i = 0; i < slides.length; i++) {
    const slide = slides[i];
    log(`▶ slide ${i + 1}/${slides.length} (${slide.id})`);
    try {
      await generateSlide({
        db,
        slide,
        slideIndex: i,
        runDir,
        implPrompt,
        managerPrompt,
        implModel,
        managerModel,
        orientation,
        force,
        log,
      });
      log(`✓ slide ${i + 1} done`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      log(`✗ slide ${i + 1} failed: ${msg}`);
      // setSlideStatus already called inside generateSlide on max-iters,
      // but guard against earlier throws (e.g. agent ran into an SDK error).
      setSlideStatus(db, slide.id, "failed", msg);
      allOk = false;
    }
  }

  let finalPdfPath: string | null = null;
  if (allOk) {
    try {
      const slug = carousel.slug;
      const runNumber = runDir.split("/run-").pop() ?? "1";
      finalPdfPath = join(runDir, `${slug}-v${runNumber}.pdf`);
      const inputs = loadLatestSlidePdfs(
        db,
        slides.map((s) => s.id),
      ).map((x) => x.pdfPath);
      log(`Concatenating ${inputs.length} slide PDFs → ${finalPdfPath}`);
      await concatPdfs(inputs, finalPdfPath);
      log(`✓ carousel.pdf written`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      log(`✗ concat failed: ${msg}`);
      allOk = false;
      finalPdfPath = null;
    }
  }

  setCarouselRunFinished(
    db,
    carouselId,
    allOk ? "done" : "failed",
    finalPdfPath,
  );
  log(allOk ? "✓ run complete" : "✗ run finished with failures");
  db.close();

  // Ensure the log line is flushed before exit.
  await new Promise((r) => setTimeout(r, 50));
}

if (import.meta.main) {
  // Per CLAUDE.md §conventions/errors: no silent failures. Anything that
  // would normally exit the bun process without a log line — uncaught
  // throws, unhandled rejections — gets a loud stderr line first. The
  // Rust supervisor's pipe-tee will capture it into run.log, and the
  // exit-watcher will flip the carousel to 'failed'.
  process.on("uncaughtException", (err) => {
    process.stderr.write(
      `[bun-driver] UNCAUGHT EXCEPTION: ${
        (err as Error)?.stack ?? String(err)
      }\n`,
    );
    process.exit(2);
  });
  process.on("unhandledRejection", (reason) => {
    process.stderr.write(
      `[bun-driver] UNHANDLED REJECTION: ${
        (reason as Error)?.stack ?? String(reason)
      }\n`,
    );
    process.exit(3);
  });

  main().catch((err) => {
    process.stderr.write(
      `[bun-driver] FATAL in main(): ${
        (err as Error)?.stack ?? String(err)
      }\n`,
    );
    process.exit(1);
  });
}
