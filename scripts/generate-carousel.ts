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
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { PDFDocument } from "pdf-lib";
import { renderSlide } from "./render-slide.ts";

// ─── Constants ──────────────────────────────────────────────────────────────

const REPO_ROOT = resolve(import.meta.dir, "..");
const DATA_DIR = join(homedir(), "Library", "Application Support", "cantalog");
const DB_PATH = join(DATA_DIR, "db.sqlite");

const IMPL_PROMPT_PATH = join(REPO_ROOT, "prompts", "implementation_agent.md");
const MGR_PROMPT_PATH = join(REPO_ROOT, "prompts", "manager_agent.md");

const MODEL = "claude-opus-4-7";

/** Maximum implement→review rounds per slide before we give up. */
const MAX_ITERS = 4;

// ─── DB helpers ─────────────────────────────────────────────────────────────

interface CarouselRow {
  id: string;
  label: string;
  slug: string | null;
  status: string;
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
      "SELECT id, label, slug, status FROM carousels WHERE id = ?",
    )
    .get(id);
  if (!row) throw new Error(`Carousel ${id} not found`);
  return row;
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

// ─── Implementation agent invocation ───────────────────────────────────────

async function driveAgent(opts: {
  cwd: string;
  systemPrompt: string;
  prompt: string;
  allowedTools: string[];
  label: string;
}): Promise<void> {
  const q = query({
    prompt: opts.prompt,
    options: {
      model: MODEL,
      cwd: opts.cwd,
      systemPrompt: opts.systemPrompt,
      permissionMode: "auto",
      allowedTools: opts.allowedTools,
    },
  });

  let resultMessage:
    | { type: "result"; subtype: string; is_error: boolean }
    | null = null;
  let messageCount = 0;
  // Log every SDK message so a stuck agent is visible in run.log instead of
  // silent. Per CLAUDE.md §conventions/errors: no silent failures.
  // We don't dump full content (that would flood the log on long thinking
  // turns); just type + a short discriminator so we can see progress.
  for await (const msg of q) {
    messageCount++;
    process.stderr.write(
      `  [sdk:${opts.label}] #${messageCount} ${describeSdkMessage(msg)}\n`,
    );
    if ((msg as { type?: string }).type === "result") {
      resultMessage = msg as typeof resultMessage;
    }
  }
  process.stderr.write(
    `  [sdk:${opts.label}] stream ended after ${messageCount} message(s)\n`,
  );
  if (!resultMessage) {
    throw new Error(
      `${opts.label} ended without a result message (got ${messageCount} message(s) total)`,
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
  });
}

interface ManagerVerdict {
  accepted: boolean;
  feedback: string;
}

async function runManager(opts: {
  slideDir: string;
  versionFilename: string; // v{N}.html
  screenshotFilename: string; // v{N}.png
  systemPrompt: string;
}): Promise<ManagerVerdict> {
  const reviewPath = join(opts.slideDir, "review.json");
  await rm(reviewPath, { force: true });

  const prompt = [
    `Working directory: ${opts.slideDir}`,
    `Review ${opts.versionFilename} (rendered to ${opts.screenshotFilename}).`,
    `Write your verdict to review.json with shape { "accepted": bool, "feedback": string }.`,
  ].join("\n\n");

  await driveAgent({
    cwd: opts.slideDir,
    systemPrompt: opts.systemPrompt,
    prompt,
    allowedTools: ["Read", "Write"],
    label: "Manager agent",
  });

  if (!existsSync(reviewPath)) {
    throw new Error("Manager agent did not write review.json");
  }
  const text = await readFile(reviewPath, "utf8");
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (e) {
    throw new Error(`Manager agent wrote invalid JSON: ${(e as Error).message}`);
  }
  if (
    typeof parsed !== "object" ||
    parsed === null ||
    typeof (parsed as { accepted?: unknown }).accepted !== "boolean" ||
    typeof (parsed as { feedback?: unknown }).feedback !== "string"
  ) {
    throw new Error(
      `Manager agent review.json missing required fields { accepted: bool, feedback: string }`,
    );
  }
  return parsed as ManagerVerdict;
}

// ─── Per-slide pipeline (Phase 3: impl + render, no manager) ───────────────

async function generateSlide(opts: {
  db: Database;
  slide: SlideRow;
  slideIndex: number;
  runDir: string;
  implPrompt: string;
  managerPrompt: string;
  log: (line: string) => void;
}) {
  const { db, slide, slideIndex, runDir, implPrompt, managerPrompt, log } = opts;
  const slideFolderName = `${String(slideIndex).padStart(2, "0")}-${slide.id}`;
  const slideDir = join(runDir, "slides", slideFolderName);
  await mkdir(slideDir, { recursive: true });

  // Freeze the source markdown at run start.
  await writeFile(join(slideDir, "source.md"), slide.content, "utf8");

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
    });
    if (!existsSync(htmlPath)) {
      throw new Error(`Implementation agent did not write ${htmlFilename}`);
    }
    const version = insertSlideVersion(db, slide.id, htmlPath);

    // Step 2 — render with puppeteer
    log(`  iter ${iter}: render`);
    await renderSlide({ htmlPath, pngPath, pdfPath });
    updateSlideVersionRenders(db, version.id, pngPath, pdfPath);

    // Step 3 — manager agent reviews
    setSlideStatus(db, slide.id, "reviewing");
    log(`  iter ${iter}: manager review`);
    const verdict = await runManager({
      slideDir,
      versionFilename: htmlFilename,
      screenshotFilename: pngFilename,
      systemPrompt: managerPrompt,
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

  log(`Starting run for carousel "${carousel.label}" (${slides.length} slide(s)).`);
  log(`Run dir: ${runDir}`);

  const implPrompt = await Bun.file(IMPL_PROMPT_PATH).text();
  const managerPrompt = await Bun.file(MGR_PROMPT_PATH).text();

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
