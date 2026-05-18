#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";
import puppeteer from "puppeteer-core";

/**
 * Per-orientation native canvas dimensions. The renderer reads the
 * carousel's orientation column and picks one of these for the puppeteer
 * viewport, screenshot clip, and PDF page size — matching the body
 * dimensions the implementation agent emits.
 */
export const ORIENTATION_DIMS = {
  vertical: { widthPx: 1080, heightPx: 1350 },
  landscape: { widthPx: 1920, heightPx: 1080 },
} as const;

export type Orientation = keyof typeof ORIENTATION_DIMS;

const CHROME_CANDIDATES = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
  "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
];

function resolveChromePath(): string {
  const fromEnv = process.env.CANTALOG_CHROME_PATH;
  if (fromEnv && existsSync(fromEnv)) return fromEnv;
  for (const candidate of CHROME_CANDIDATES) {
    if (existsSync(candidate)) return candidate;
  }
  throw new Error(
    "No Chrome/Chromium found. Install Google Chrome, or set CANTALOG_CHROME_PATH.",
  );
}

export interface RenderResult {
  htmlPath: string;
  pngPath: string;
  pdfPath: string;
}

export async function renderSlide(opts: {
  htmlPath: string;
  pngPath: string;
  pdfPath: string;
  chromePath?: string;
  /**
   * Carousel orientation. Defaults to `vertical` so callers that
   * predate the landscape feature keep producing 1080×1350 output.
   */
  orientation?: Orientation;
}): Promise<RenderResult> {
  const htmlPath = isAbsolute(opts.htmlPath)
    ? opts.htmlPath
    : resolve(opts.htmlPath);
  if (!existsSync(htmlPath)) {
    throw new Error(`HTML not found: ${htmlPath}`);
  }
  const pngPath = isAbsolute(opts.pngPath) ? opts.pngPath : resolve(opts.pngPath);
  const pdfPath = isAbsolute(opts.pdfPath) ? opts.pdfPath : resolve(opts.pdfPath);

  await mkdir(dirname(pngPath), { recursive: true });
  await mkdir(dirname(pdfPath), { recursive: true });

  const executablePath = opts.chromePath ?? resolveChromePath();
  const { widthPx, heightPx } = ORIENTATION_DIMS[opts.orientation ?? "vertical"];

  const browser = await puppeteer.launch({
    executablePath,
    headless: true,
    args: ["--no-sandbox", "--disable-setuid-sandbox"],
  });

  try {
    const page = await browser.newPage();
    await page.setViewport({
      width: widthPx,
      height: heightPx,
      deviceScaleFactor: 2,
    });
    await page.goto(`file://${htmlPath}`, { waitUntil: "networkidle0" });
    // Wait for web fonts to settle so text doesn't render as system fallback.
    await page.evaluate(() => (document as Document & { fonts: { ready: Promise<void> } }).fonts.ready);

    await page.screenshot({
      path: pngPath as `${string}.png`,
      type: "png",
      clip: { x: 0, y: 0, width: widthPx, height: heightPx },
    });

    await page.pdf({
      path: pdfPath,
      width: `${widthPx}px`,
      height: `${heightPx}px`,
      printBackground: true,
      pageRanges: "1",
      preferCSSPageSize: false,
      margin: { top: 0, right: 0, bottom: 0, left: 0 },
    });
  } finally {
    await browser.close();
  }

  return { htmlPath, pngPath, pdfPath };
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 3) {
    console.error(
      "Usage: bun run scripts/render-slide.ts <input.html> <out.png> <out.pdf> [orientation]",
    );
    process.exit(1);
  }
  const [input, outPng, outPdf, orientationArg] = args;
  let orientation: Orientation | undefined;
  if (orientationArg) {
    if (orientationArg !== "vertical" && orientationArg !== "landscape") {
      console.error(
        `Unknown orientation: ${orientationArg} (expected vertical|landscape)`,
      );
      process.exit(1);
    }
    orientation = orientationArg;
  }
  const result = await renderSlide({
    htmlPath: input,
    pngPath: outPng,
    pdfPath: outPdf,
    orientation,
  });
  console.log(JSON.stringify(result, null, 2));
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
