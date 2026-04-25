import { describe, expect, it } from "vitest";
import { defaultSlideTitle, effectiveSlideTitle } from "./slide-title";

describe("defaultSlideTitle", () => {
  it("returns null for empty content", () => {
    expect(defaultSlideTitle("")).toBeNull();
    expect(defaultSlideTitle("   \n\n  ")).toBeNull();
  });

  it("strips a leading # header", () => {
    expect(defaultSlideTitle("# Where Opus 4.7 Excels\n\nbody")).toBe(
      "Where Opus 4.7 Excels",
    );
  });

  it("strips multi-hash headers", () => {
    expect(defaultSlideTitle("## Subhead\n\nbody")).toBe("Subhead");
    expect(defaultSlideTitle("### Smaller still")).toBe("Smaller still");
  });

  it("falls back to the first non-empty line", () => {
    expect(defaultSlideTitle("Plate II — The Problem\n\nbody")).toBe(
      "Plate II — The Problem",
    );
  });

  it("handles \\r line endings", () => {
    expect(defaultSlideTitle("# windows-style\r\nbody")).toBe(
      "windows-style",
    );
    expect(defaultSlideTitle("classic mac\rbody")).toBe("classic mac");
  });

  it("skips leading blank lines", () => {
    expect(defaultSlideTitle("\n\n# After blanks\n")).toBe("After blanks");
  });

  it("returns null for a header with no text", () => {
    expect(defaultSlideTitle("#\n\nbody")).toBe("body");
    expect(defaultSlideTitle("#  \n")).toBeNull();
  });
});

describe("effectiveSlideTitle", () => {
  it("prefers an explicit title", () => {
    expect(effectiveSlideTitle("Custom", "# Auto", 0)).toBe("Custom");
  });

  it("trims an explicit title", () => {
    expect(effectiveSlideTitle("  Spaced  ", "# Auto", 0)).toBe("Spaced");
  });

  it("falls back to the auto-derived title when explicit is null", () => {
    expect(effectiveSlideTitle(null, "# Auto", 0)).toBe("Auto");
  });

  it("falls back to the auto-derived title when explicit is whitespace", () => {
    expect(effectiveSlideTitle("   ", "# Auto", 0)).toBe("Auto");
  });

  it("falls back to Slide N when both explicit and content are empty", () => {
    expect(effectiveSlideTitle(null, "", 2)).toBe("Slide 3");
  });
});
