/**
 * Pick a default title for a slide from its markdown content.
 *
 * Rule (from the user spec):
 *   - If the first non-empty line is a `# header`, return the header text
 *     (with the `# ` and any extra leading hashes stripped).
 *   - Otherwise return that first non-empty line as-is, up to the first
 *     return / carriage return.
 *   - If the content has no non-empty lines, return null so the caller
 *     can fall back to "Slide N".
 *
 * The title is shown in the editor tab list and the progress view, so
 * we trim aggressively — don't preserve trailing whitespace.
 */
export function defaultSlideTitle(content: string): string | null {
  if (!content) return null;
  // Match either \n or \r as line breaks per spec.
  for (const rawLine of content.split(/\r?\n|\r/)) {
    const line = rawLine.trim();
    if (line.length === 0) continue;
    // Strip a leading run of `#`s + their whitespace so `# X`, `## X`, and
    // a non-markdown plain line all collapse to the same shape. If the
    // result is empty (line was just `#` or `## `), skip and try the next
    // non-empty line — we never want a bare hash as a slide title.
    const stripped = line.replace(/^#+\s*/, "").trim();
    if (stripped.length === 0) continue;
    return stripped;
  }
  return null;
}

/** Title to show in UI: explicit user title, else default, else "Slide N". */
export function effectiveSlideTitle(
  title: string | null,
  content: string,
  fallbackIndex: number,
): string {
  const trimmedExplicit = title?.trim();
  if (trimmedExplicit) return trimmedExplicit;
  const computed = defaultSlideTitle(content);
  if (computed) return computed;
  return `Slide ${fallbackIndex + 1}`;
}
