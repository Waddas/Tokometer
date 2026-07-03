// E2E smoke test: clicking the widget's settings button must open the
// settings window and leave the app responsive. Guards the class of bug where
// window creation inside an IPC callback deadlocks WebView2 on Windows
// (wry#583) — unit tests can't see it because it needs the real event loop.
//
// Drives the app over the Chrome DevTools Protocol, which WebView2 exposes
// when launched with --remote-debugging-port, so it runs on Windows only.
// Build the app first:
//   npm run tauri build -- --debug --no-bundle --config src-tauri/tauri.e2e.conf.json
// then: npm run test:e2e
//
// The e2e config overlay gives the app its own identifier so the test never
// touches a real install's state or single-instance lock.

import { spawn, execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const PORT = 9223;
const APP_START_TIMEOUT_MS = 30_000;
const SETTINGS_TIMEOUT_MS = 15_000;

const root = fileURLToPath(new URL("..", import.meta.url));
const exe = join(root, "src-tauri", "target", "debug", "tokometer.exe");

if (process.platform !== "win32") {
  console.log("skip: WebView2 CDP driving is Windows-only");
  process.exit(0);
}
if (!existsSync(exe)) {
  console.error(`app binary not found: ${exe}`);
  console.error(
    "build it first: npm run tauri build -- --debug --no-bundle --config src-tauri/tauri.e2e.conf.json",
  );
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Poll fn until it returns a truthy value or the deadline passes. */
async function waitFor(what, timeoutMs, fn) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await fn().catch(() => null);
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await sleep(250);
  }
}

/** The CDP page targets currently alive ({url, webSocketDebuggerUrl}). */
async function pages() {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  const targets = await res.json();
  return targets.filter((t) => t.type === "page");
}

/** Evaluate an expression in a page; promises are awaited to their value. */
async function evaluate(page, expression) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("CDP websocket failed"));
  });
  try {
    const reply = new Promise((resolve, reject) => {
      ws.onmessage = (event) => resolve(JSON.parse(event.data));
      setTimeout(() => reject(new Error(`evaluate timed out: ${expression}`)), 5_000);
    });
    ws.send(
      JSON.stringify({
        id: 1,
        method: "Runtime.evaluate",
        params: { expression, returnByValue: true, awaitPromise: true },
      }),
    );
    const { result } = await reply;
    if (result?.exceptionDetails) {
      throw new Error(`evaluate threw: ${result.exceptionDetails.text}`);
    }
    return result?.result?.value;
  } finally {
    ws.close();
  }
}

console.log("launching app with CDP enabled…");
const app = spawn(exe, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

let failed = false;
try {
  // The widget page is served from the app origin's root.
  const main = await waitFor("main window page", APP_START_TIMEOUT_MS, async () => {
    const all = await pages();
    return all.find((p) => !p.url.includes("settings"));
  });
  console.log(`main window up: ${main.url}`);

  // main.ts is a module script, so by readyState "complete" its listeners are attached.
  await waitFor("widget UI ready", APP_START_TIMEOUT_MS, () =>
    evaluate(
      main,
      "document.readyState === 'complete' && !!document.getElementById('btn-settings')",
    ),
  );

  console.log("clicking the settings button…");
  await evaluate(main, "document.getElementById('btn-settings').click(); true");

  const settings = await waitFor("settings window page", SETTINGS_TIMEOUT_MS, async () => {
    const all = await pages();
    return all.find((p) => p.url.includes("settings"));
  });
  console.log(`settings window up: ${settings.url}`);

  // A second click must not crash or spawn a duplicate — it refocuses.
  await evaluate(main, "document.getElementById('btn-settings').click(); true");
  await sleep(1_000);
  const settingsPages = (await pages()).filter((p) => p.url.includes("settings"));
  if (settingsPages.length !== 1) {
    throw new Error(`expected 1 settings page after second click, got ${settingsPages.length}`);
  }

  // The backend must still answer IPC — a deadlocked main thread wouldn't.
  await evaluate(main, "window.__TAURI_INTERNALS__.invoke('get_state').then(() => true)");

  console.log("PASS: settings window opens and the app stays responsive");
} catch (err) {
  failed = true;
  console.error(`FAIL: ${err.message}`);
} finally {
  // Kill the whole tree — WebView2 spawns child processes.
  try {
    execSync(`taskkill /PID ${app.pid} /T /F`, { stdio: "ignore" });
  } catch {
    // already gone
  }
}
process.exit(failed ? 1 : 0);
