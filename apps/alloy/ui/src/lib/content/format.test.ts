import { describe, expect, it, vi } from "vitest";

vi.mock("../ipc/index.js", () => ({ client: { invoke: vi.fn() } }));

import { formatListTime, formatTime, isEmojiOnly } from "./format";

const afternoon = new Date(2026, 9, 1, 15, 5).getTime();

describe("formatTime", () => {
  it("forces a 24-hour clock", () => {
    expect(formatTime(afternoon, "24h")).toBe("15:05");
  });

  it("forces a 12-hour clock", () => {
    expect(formatTime(afternoon, "12h")).toMatch(/^3:05\s?PM$/i);
  });

  it("follows the locale for auto", () => {
    const expected = new Date(afternoon).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    expect(formatTime(afternoon, "auto")).toBe(expected);
  });
});

describe("formatListTime", () => {
  it("uses the clock pref for today's stamps", () => {
    const now = new Date(2026, 9, 1, 20, 0);
    expect(formatListTime(afternoon, now, "24h")).toBe("15:05");
    expect(formatListTime(afternoon, now, "12h")).toMatch(/^3:05\s?PM$/i);
  });
});

describe("isEmojiOnly", () => {
  it("accepts one to three emoji", () => {
    expect(isEmojiOnly("\u{1F600}")).toBe(true);
    expect(isEmojiOnly("\u{1F600}\u{1F600}\u{1F600}")).toBe(true);
    expect(isEmojiOnly("\u{1F389} \u{1F602}")).toBe(true);
  });

  it("rejects more than the limit", () => {
    expect(isEmojiOnly("\u{1F600}\u{1F600}\u{1F600}\u{1F600}")).toBe(false);
    expect(isEmojiOnly("\u{1F600}\u{1F600}", 1)).toBe(false);
  });

  it("counts multi-codepoint sequences as one emoji", () => {
    expect(isEmojiOnly("\u{1F468}\u200D\u{1F469}\u200D\u{1F467}")).toBe(true);
    expect(isEmojiOnly("\u{1F1EB}\u{1F1F7}")).toBe(true);
    expect(isEmojiOnly("\u{1F44D}\u{1F3FD}")).toBe(true);
    expect(isEmojiOnly("1\uFE0F\u20E3")).toBe(true);
  });

  it("rejects text, mixed content and empty input", () => {
    expect(isEmojiOnly("hi \u{1F600}")).toBe(false);
    expect(isEmojiOnly("\u{1F600} a")).toBe(false);
    expect(isEmojiOnly("1")).toBe(false);
    expect(isEmojiOnly("")).toBe(false);
    expect(isEmojiOnly("   ")).toBe(false);
  });
});
