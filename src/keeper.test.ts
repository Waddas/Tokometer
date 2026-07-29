import { describe, expect, it } from "vitest";
import { keeperStatus } from "./keeper";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const NOW = 1_780_596_000_000;

describe("keeperStatus", () => {
  it("says nothing while the keeper is off", () => {
    expect(keeperStatus(false, null, NOW)).toBe("");
    expect(keeperStatus(false, NOW - HOUR, NOW)).toBe("");
  });

  it("reports when it has never run", () => {
    expect(keeperStatus(true, null, NOW)).toBe("Not run yet.");
  });

  it("calls a run inside the last minute 'just now'", () => {
    expect(keeperStatus(true, NOW, NOW)).toBe("Last run just now.");
    expect(keeperStatus(true, NOW - MINUTE + 1, NOW)).toBe("Last run just now.");
  });

  it("counts in minutes under an hour", () => {
    expect(keeperStatus(true, NOW - MINUTE, NOW)).toBe("Last run 1m ago.");
    expect(keeperStatus(true, NOW - 59 * MINUTE, NOW)).toBe("Last run 59m ago.");
  });

  it("counts in hours and minutes under a day", () => {
    expect(keeperStatus(true, NOW - HOUR, NOW)).toBe("Last run 1h 0m ago.");
    expect(keeperStatus(true, NOW - (2 * HOUR + 13 * MINUTE), NOW)).toBe("Last run 2h 13m ago.");
  });

  it("counts in days and hours beyond a day", () => {
    expect(keeperStatus(true, NOW - DAY, NOW)).toBe("Last run 1d 0h ago.");
    expect(keeperStatus(true, NOW - (3 * DAY + 5 * HOUR), NOW)).toBe("Last run 3d 5h ago.");
  });

  it("treats a future timestamp as just now rather than negative time", () => {
    // Can happen after the system clock moves backwards.
    expect(keeperStatus(true, NOW + HOUR, NOW)).toBe("Last run just now.");
  });
});
