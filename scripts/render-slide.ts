#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";
import puppeteer from "puppeteer-core";

const SLIDE_WIDTH_PX = 1080;
const SLIDE_HEIGHT_PX = 1350;

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

  const browser = await puppeteer.launch({
    executablePath,
    headless: true,
    args: ["--no-sandbox", "--disable-setuid-sandbox"],
  });

  try {
    const page = await browser.newPage();
    await page.setViewport({
      width: SLIDE_WIDTH_PX,
      height: SLIDE_HEIGHT_PX,
      deviceScaleFactor: 2,
    });
    await page.goto(`file://${htmlPath}`, { waitUntil: "networkidle0" });
    // Wait for web fonts to settle so text doesn't render as system fallback.
    await page.evaluate(() => (document as Document & { fonts: { ready: Promise<void> } }).fonts.ready);

    await page.screenshot({
      path: pngPath as `${string}.png`,
      type: "png",
      clip: { x: 0, y: 0, width: SLIDE_WIDTH_PX, height: SLIDE_HEIGHT_PX },
    });

    await page.pdf({
      path: pdfPath,
      width: `${SLIDE_WIDTH_PX}px`,
      height: `${SLIDE_HEIGHT_PX}px`,
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
      "Usage: bun run scripts/render-slide.ts <input.html> <out.png> <out.pdf>",
    );
    process.exit(1);
  }
  const [input, outPng, outPdf] = args;
  const result = await renderSlide({
    htmlPath: input,
    pngPath: outPng,
    pdfPath: outPdf,
  });
  console.log(JSON.stringify(result, null, 2));
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
