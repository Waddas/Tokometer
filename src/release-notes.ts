// Release notes as the updater manifest carries them: the release-please
// changelog entry for the version, in markdown. The settings card shows the
// change groups and their bullets; the version heading, blank lines and the
// trailing issue/commit links are noise there and are dropped.

export interface NotesGroup {
  /** "Features", "Bug Fixes"…; empty when the bullets had no heading. */
  heading: string;
  items: string[];
}

const HEADING = /^#{3,}\s+(.+)$/;
const BULLET = /^[*-]\s+(.+)$/;
/** `[text](url)` → `text`. */
const LINK = /\[([^\]]*)\]\([^)]*\)/g;
/** The `(#16) (4c8dd52)` refs release-please appends, once their links are gone. */
const REFS = /(\s*\((?:#\d+|[0-9a-f]{7,40})\))+$/;

export function parseReleaseNotes(markdown: string): NotesGroup[] {
  const groups: NotesGroup[] = [];
  for (const raw of markdown.split(/\r?\n/)) {
    const line = raw.trim();
    const heading = HEADING.exec(line);
    if (heading) {
      groups.push({ heading: heading[1].trim(), items: [] });
      continue;
    }
    const bullet = BULLET.exec(line);
    if (!bullet) continue;
    const text = bullet[1].replace(LINK, "$1").replace(REFS, "").replace(/\*\*/g, "").trim();
    if (text === "") continue;
    if (groups.length === 0) groups.push({ heading: "", items: [] });
    groups[groups.length - 1].items.push(text);
  }
  return groups.filter((g) => g.items.length > 0);
}
