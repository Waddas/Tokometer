import { defineConfig } from "vitest/config";

// Frontend unit tests. happy-dom gives the DOM APIs UsageRenderer needs;
// `@tauri-apps/api` is mocked per-test so nothing touches the IPC bridge.
export default defineConfig({
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
    globals: true,
  },
});
