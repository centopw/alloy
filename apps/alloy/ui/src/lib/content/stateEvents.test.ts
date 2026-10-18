import { describe, expect, it } from "vitest";

import { isHiddenState, stateEventSummary } from "./stateEvents";

const allVisible = { showMembershipEvents: true, showProfileChanges: true, showRoomUpdates: true };
const allHidden = { showMembershipEvents: false, showProfileChanges: false, showRoomUpdates: false };

describe("stateEventSummary", () => {
  it("returns the summary field when present", () => {
    expect(stateEventSummary({ content_json: JSON.stringify({ summary: "Ana joined" }) })).toBe("Ana joined");
  });

  it("returns null for empty, missing or malformed summaries", () => {
    expect(stateEventSummary({ content_json: JSON.stringify({ summary: "" }) })).toBeNull();
    expect(stateEventSummary({ content_json: JSON.stringify({ name: "x" }) })).toBeNull();
    expect(stateEventSummary({ content_json: JSON.stringify({ summary: 3 }) })).toBeNull();
    expect(stateEventSummary({ content_json: "not json" })).toBeNull();
    expect(stateEventSummary({ content_json: "null" })).toBeNull();
  });
});

describe("isHiddenState", () => {
  it("shows everything when all prefs are on", () => {
    for (const type of ["membership", "profile", "name", "topic", "avatar", "encryption", "state"]) {
      expect(isHiddenState(type, allVisible)).toBe(false);
    }
  });

  it("hides membership only when its pref is off", () => {
    expect(isHiddenState("membership", { ...allVisible, showMembershipEvents: false })).toBe(true);
    expect(isHiddenState("membership", { ...allHidden, showMembershipEvents: true })).toBe(false);
  });

  it("hides profile changes only when their pref is off", () => {
    expect(isHiddenState("profile", { ...allVisible, showProfileChanges: false })).toBe(true);
    expect(isHiddenState("profile", { ...allHidden, showProfileChanges: true })).toBe(false);
  });

  it("governs every other state type with the room updates pref", () => {
    for (const type of ["name", "topic", "avatar", "encryption", "state", "unsupported", "something-new"]) {
      expect(isHiddenState(type, { ...allVisible, showRoomUpdates: false })).toBe(true);
      expect(isHiddenState(type, { ...allHidden, showRoomUpdates: true })).toBe(false);
    }
  });
});
