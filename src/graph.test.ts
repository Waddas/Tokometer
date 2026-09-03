import { describe, expect, it } from "vitest";
import { windowSpan } from "./graph";
import { SESSION_ID, WEEKLY_ALL_ID } from "./api";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

describe("windowSpan", () => {
  it("spans the session window five hours", () => {
    expect(windowSpan(SESSION_ID).windowMs).toBe(5 * HOUR);
  });

  it("spans weekly windows, scoped or not, seven days", () => {
    expect(windowSpan(WEEKLY_ALL_ID).windowMs).toBe(7 * DAY);
    expect(windowSpan("weekly_scoped:fable").windowMs).toBe(7 * DAY);
  });

  it("spans monthly kinds thirty days and unknown kinds a week", () => {
    expect(windowSpan("monthly_all").windowMs).toBe(30 * DAY);
    expect(windowSpan("monthly_scoped:opus").windowMs).toBe(30 * DAY);
    expect(windowSpan("daily_quota").windowMs).toBe(7 * DAY);
  });
});
