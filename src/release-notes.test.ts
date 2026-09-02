import { describe, expect, it } from "vitest";
import { parseReleaseNotes } from "./release-notes";

const RELEASE_PLEASE_BODY = `## [1.3.0](https://github.com/Waddas/Tokometer/compare/v1.2.2...v1.3.0) (2026-07-29)


### Features

* auto-detect usage limits and render a tile per limit ([#16](https://github.com/Waddas/Tokometer/issues/16)) ([4c8dd52](https://github.com/Waddas/Tokometer/commit/4c8dd520e2d40fc4eaa9e01232af1b42d7d01003))
* **tray:** ring or text icon ([abc1234](https://github.com/Waddas/Tokometer/commit/abc1234))


### Bug Fixes

- skip self-update checks in dev builds ([#12](https://github.com/Waddas/Tokometer/issues/12))
`;

describe("parseReleaseNotes", () => {
  it("keeps the change groups and bullets, dropping links and commit refs", () => {
    expect(parseReleaseNotes(RELEASE_PLEASE_BODY)).toEqual([
      {
        heading: "Features",
        items: [
          "auto-detect usage limits and render a tile per limit",
          "tray: ring or text icon",
        ],
      },
      { heading: "Bug Fixes", items: ["skip self-update checks in dev builds"] },
    ]);
  });

  it("groups headingless bullets and ignores prose", () => {
    expect(parseReleaseNotes("Some intro text\n\n* one\n* two\n")).toEqual([
      { heading: "", items: ["one", "two"] },
    ]);
  });

  it("returns nothing for empty or bullet-free notes", () => {
    expect(parseReleaseNotes("")).toEqual([]);
    expect(parseReleaseNotes("### Features\n\nno bullets here")).toEqual([]);
  });
});
