// The one definition of the usage-severity thresholds: tiles turn amber/red
// (usage.ts) and the graph gradient blends (graph.ts) at the same points.
// Keep in sync with the tray ring's ramp in src-tauri/src/trayicon.rs.
export const AMBER_AT_PCT = 50;
export const RED_AT_PCT = 80;
