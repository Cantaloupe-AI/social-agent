#!/usr/bin/env bun
/**
 * Carousel generation driver — Phase 3 (implementation agent only).
 *
 * Spawned by the Tauri backend with one argument: the carousel id.
 * Reads the cantalog SQLite db directly via bun:sqlite and writes status
 * updates as it goes. The frontend polls the same db to display progress.
 *
 * Phase 4 will add the manager agent + iteration loop. For now: one
 * implementation pass per slide, then the slide is marked accepted and
 * we move on. PDF concatenation comes in Phase 5.
 */

import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { appendFile, mkdir, readdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { renderSlide } from "./render-slide.ts";

// ─── Constants ──────────────────────────────────────────────────────────────

const REPO_ROOT = resolve(import.meta.dir, "..");
const DATA_DIR = join(homedir(), "Library", "Application Support", "cantalog");
const DB_PATH = join(DATA_DIR, "db.sqlite");
const CAROUSELS_DIR = join(DATA_DIR, "carousels");

const IMPL_PROMPT_PATH = join(REPO_ROOT, "prompts", "implementation_agent.md");

const MODEL = "claude-opus-4-7";

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

function setCarouselRunStarted(db: Database, id: string, runDir: string) {
  const now = new Date().toISOString();
  db.run(
    `UPDATE carousels
     SET status = 'generating', run_dir = ?, run_started_at = ?, run_finished_at = NULL,
         pdf_path = NULL, updated_at = ?
     WHERE id = ?`,
    [runDir, now, now, id],
  );
}

function setCarouselRunFinished(
  db: Database,
  id: string,
  status: "done" | "failed",
) {
  const now = new Date().toISOString();
  db.run(
    `UPDATE carousels
     SET status = ?, run_finished_at = ?, updated_at = ?
     WHERE id = ?`,
    [status, now, now, id],
  );
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

// ─── Run-dir resolution (no overwriting existing runs) ─────────────────────

async function nextRunDir(slug: string): Promise<string> {
  const carouselDir = join(CAROUSELS_DIR, slug);
  await mkdir(carouselDir, { recursive: true });
  const existing = await readdir(carouselDir);
  const nums = existing
    .filter((name) => name.startsWith("run-"))
    .map((name) => Number.parseInt(name.slice(4), 10))
    .filter((n) => Number.isFinite(n));
  const next = (nums.length === 0 ? 0 : Math.max(...nums)) + 1;
  const dir = join(carouselDir, `run-${next}`);
  if (existsSync(dir)) {
    throw new Error(`Run dir already exists: ${dir}`);
  }
  await mkdir(dir, { recursive: false });
  return dir;
}

// ─── Implementation agent invocation ───────────────────────────────────────

async function runImplementer(opts: {
  slideDir: string;
  versionFilename: string; // e.g. "v1.html"
  systemPrompt: string;
  iterationFeedback: string | null;
}): Promise<void> {
  const promptParts = [
    `You are running for slide working directory: ${opts.slideDir}`,
    `Write your output to: ${opts.versionFilename}`,
    `Read source.md for the slide markdown.`,
  ];
  if (opts.iterationFeedback) {
    promptParts.push(
      `feedback.md exists with this manager review — address every point:\n${opts.iterationFeedback}`,
    );
  } else {
    promptParts.push(
      `This is the first iteration; feedback.md does not exist yet.`,
    );
  }
  const prompt = promptParts.join("\n\n");

  const q = query({
    prompt,
    options: {
      model: MODEL,
      cwd: opts.slideDir,
      systemPrompt: opts.systemPrompt,
      permissionMode: "auto",
      allowedTools: ["Read", "Write", "Edit"],
    },
  });

  let resultMessage:
    | { type: "result"; subtype: string; is_error: boolean }
    | null = null;
  for await (const msg of q) {
    if (msg.type === "result") {
      resultMessage = msg as typeof resultMessage;
    }
  }
  if (!resultMessage) {
    throw new Error("Implementation agent ended without a result message");
  }
  if (resultMessage.is_error) {
    throw new Error(
      `Implementation agent errored: ${resultMessage.subtype}`,
    );
  }
}

// ─── Per-slide pipeline (Phase 3: impl + render, no manager) ───────────────

async function generateSlide(opts: {
  db: Database;
  slide: SlideRow;
  slideIndex: number;
  runDir: string;
  systemPrompt: string;
}) {
  const { db, slide, slideIndex, runDir, systemPrompt } = opts;
  const slideFolderName = `${String(slideIndex).padStart(2, "0")}-${slide.id}`;
  const slideDir = join(runDir, "slides", slideFolderName);
  await mkdir(slideDir, { recursive: true });

  // Freeze the source markdown at run start.
  await writeFile(join(slideDir, "source.md"), slide.content, "utf8");

  setSlideStatus(db, slide.id, "generating");

  const versionFilename = "v1.html";
  const htmlPath = join(slideDir, versionFilename);
  const pngPath = join(slideDir, "v1.png");
  const pdfPath = join(slideDir, "v1.pdf");

  await runImplementer({
    slideDir,
    versionFilename,
    systemPrompt,
    iterationFeedback: null,
  });

  if (!existsSync(htmlPath)) {
    throw new Error(`Implementation agent did not write ${versionFilename}`);
  }

  const version = insertSlideVersion(db, slide.id, htmlPath);

  // Render with puppeteer
  await renderSlide({ htmlPath, pngPath, pdfPath });
  updateSlideVersionRenders(db, version.id, pngPath, pdfPath);

  // Phase 3 stops here — no manager review yet. Mark accepted as a placeholder
  // so the user sees the slide finished. Phase 4 will replace this with a real
  // accept/reject loop driven by the manager agent.
  setSlideStatus(db, slide.id, "accepted");
}

// ─── Main ──────────────────────────────────────────────────────────────────

async function main() {
  const carouselId = process.argv[2];
  if (!carouselId) {
    console.error("Usage: bun run scripts/generate-carousel.ts <carousel_id>");
    process.exit(1);
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

  const runDir = await nextRunDir(carousel.slug);
  setCarouselRunStarted(db, carouselId, runDir);

  // Append-only run log for debugging.
  const logPath = join(runDir, "run.log");
  const log = (line: string) => {
    const stamped = `[${new Date().toISOString()}] ${line}\n`;
    process.stderr.write(stamped);
    appendFile(logPath, stamped, "utf8").catch(() => {});
  };

  log(`Starting run for carousel "${carousel.label}" (${slides.length} slide(s)).`);
  log(`Run dir: ${runDir}`);

  const systemPrompt = await Bun.file(IMPL_PROMPT_PATH).text();

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
        systemPrompt,
      });
      log(`✓ slide ${i + 1} accepted (Phase 3: no manager review yet)`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      log(`✗ slide ${i + 1} failed: ${msg}`);
      setSlideStatus(db, slide.id, "failed", msg);
      allOk = false;
    }
  }

  setCarouselRunFinished(db, carouselId, allOk ? "done" : "failed");
  log(allOk ? "✓ run complete" : "✗ run finished with failures");
  db.close();

  // Ensure the log line is flushed before exit.
  await new Promise((r) => setTimeout(r, 50));
}

if (import.meta.main) {
  main().catch((err) => {
    console.error("Fatal:", err);
    process.exit(1);
  });
}
