import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { applyTheme, restoreTheme } from "./theme";

describe("theme first paint", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  afterEach(() => vi.restoreAllMocks());

  it("uses Charcoal when there is no cached preference or the cached id is retired", () => {
    restoreTheme();
    expect(document.documentElement.dataset.theme).toBe("charcoal");
    localStorage.setItem("appearance-theme", "retired-theme");
    restoreTheme();
    expect(document.documentElement.dataset.theme).toBe("charcoal");
  });

  it("restores a light theme before the backend responds", () => {
    applyTheme("paper");
    delete document.documentElement.dataset.theme;
    restoreTheme();
    expect(document.documentElement.dataset.theme).toBe("paper");
  });

  it("lets the backend preference replace a stale first-paint cache", () => {
    localStorage.setItem("appearance-theme", "midnight");
    restoreTheme();
    applyTheme("mist");
    expect(document.documentElement.dataset.theme).toBe("mist");
    expect(localStorage.getItem("appearance-theme")).toBe("mist");
  });

  it("still applies backend preferences when browser storage is unavailable", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("Storage unavailable");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("Storage unavailable");
    });
    restoreTheme();
    expect(document.documentElement.dataset.theme).toBe("charcoal");
    applyTheme("paper");
    expect(document.documentElement.dataset.theme).toBe("paper");
  });
});
