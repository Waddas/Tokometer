// Selectable mascots: each is a set of palette-indexed animations keyed by the
// names splash.ts groups by usage rate. Ids mirror the Rust `Mascot` enum.
import { ANIMATIONS, type ClawdAnimation } from "./animations";
import { AXOLOTL_ANIMATIONS } from "./axolotl";
import { CAT_ANIMATIONS } from "./cat";

export type MascotId = "clawd" | "axolotl" | "cat";

export const MASCOTS: Record<MascotId, Record<string, ClawdAnimation>> = {
  clawd: ANIMATIONS,
  axolotl: AXOLOTL_ANIMATIONS,
  cat: CAT_ANIMATIONS,
};

export const DEFAULT_MASCOT: MascotId = "clawd";
