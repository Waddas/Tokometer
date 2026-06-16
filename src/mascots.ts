// Selectable mascots: each is a set of palette-indexed animations keyed by the
// names splash.ts groups by usage rate. Ids mirror the Rust `Mascot` enum.
import {
  CLAWD_ANIMATIONS,
  AXOLOTL_ANIMATIONS,
  CAT_ANIMATIONS,
  type MascotAnimation,
} from "./animations";

export type MascotId = "clawd" | "axolotl" | "cat";

export const MASCOTS: Record<MascotId, Record<string, MascotAnimation>> = {
  clawd: CLAWD_ANIMATIONS,
  axolotl: AXOLOTL_ANIMATIONS,
  cat: CAT_ANIMATIONS,
};

export const DEFAULT_MASCOT: MascotId = "clawd";
