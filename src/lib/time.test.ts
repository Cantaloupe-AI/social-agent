import { describe, expect, it } from "vitest";
import { formatClockTime, formatFriendlyDate } from "./time";

describe("formatClockTime", () => {
  it("formats an ISO timestamp as zero-padded HH:MM", () => {
    // Use a local-tz constructor so the test is stable across timezones.
    const d = new Date(2026, 3, 19, 9, 5);
    expect(formatClockTime(d.toISOString())).toBe("09:05");
  });

  it("returns --:-- for invalid input", () => {
    expect(formatClockTime("not an iso")).toBe("--:--");
  });
});

describe("formatFriendlyDate", () => {
  it("renders weekday, long month, and day", () => {
    const d = new Date(2026, 3, 19); // April 19, 2026 is a Sunday
    expect(formatFriendlyDate(d)).toBe("Sunday, April 19");
  });
});
