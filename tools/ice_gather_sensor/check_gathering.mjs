// Manual sensor: what does a browser gather with naia's exact ICE config?
//
// Usage:
//   node check_gathering.mjs [path-to-gather_probe.html]
//
// Needs headless google-chrome (any remote-debugging-capable Chromium) and
// node >= 22 (global WebSocket). No npm installs.
//
// Exit codes are the whole contract:
//   0  non-vacuous gather, no srflx/relay  (absence, answerable)
//   1  srflx or relay gathered             (presence, self-evidencing)
//   2  BLIND -- gathering never completed, yielded nothing, Chrome missing,
//      or the result was unparsable. UNKNOWN, never PASS.
//
// Deliberately NOT wired into any cargo gate: CI builds the wasm target but
// runs no browser, so a gated version would either skip silently or count as
// coverage while running zero tests. Run by hand when the ICE config or the
// offer path changes.
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const HTML = resolve(process.argv[2] ?? join(HERE, "gather_probe.html"));
const PORT = 9371;
const DEADLINE_MS = 30000;

function blind(reason) {
  console.log(`BLIND: ${reason}`);
  process.exit(2);
}

let chrome;
try {
  chrome = spawn(
    "google-chrome",
    ["--headless=new", "--no-first-run", "--disable-gpu",
     `--remote-debugging-port=${PORT}`, "--remote-allow-origins=*", "about:blank"],
    { stdio: "ignore" }
  );
} catch {
  blind("could not spawn google-chrome");
}
chrome.on("error", () => blind("could not spawn google-chrome (absent?)"));
const kill = () => { try { chrome.kill("SIGKILL"); } catch {} };
process.on("exit", kill);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function waitForEndpoint(tries = 50) {
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/version`);
      if (r.ok) return;
    } catch {}
    await sleep(200);
  }
  blind("chrome remote-debugging endpoint never came up");
}

await waitForEndpoint();
const tab = await (await fetch(`http://127.0.0.1:${PORT}/json/new?file://${HTML}`, {
  method: "PUT",
})).json();
const ws = new WebSocket(tab.webSocketDebuggerUrl, { maxPayload: 64 * 1024 * 1024 });
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

let id = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) pending.get(m.id)(m);
};
const send = (method, params = {}) =>
  new Promise((res) => {
    const mid = ++id;
    pending.set(mid, res);
    ws.send(JSON.stringify({ id: mid, method, params }));
  });

await send("Page.enable");
await send("Page.navigate", { url: `file://${HTML}` });

let report = null;
const t0 = Date.now();
while (Date.now() - t0 < DEADLINE_MS) {
  await sleep(500);
  const r = await send("Runtime.evaluate", {
    expression: "document.getElementById('result').textContent",
    returnByValue: true,
  });
  const text = r?.result?.result?.value ?? "";
  if (!text.includes("pending") && text.startsWith("{")) {
    try { report = JSON.parse(text); break; } catch {}
  }
}
ws.close();
kill();

if (!report) blind("page never reported within deadline");
const cands = report.candidates ?? [];
console.log(`gatheringState=${report.gatheringState} blind=${report.blind} ` +
  `error=${report.error} ncandidates=${cands.length}`);
for (const c of cands) console.log("  cand:", c);
const posted = report.postedSdp ?? "";
const postedCands = posted.split("\n").filter((l) => l.includes("a=candidate:"));
console.log(`posted-offer-shape candidate lines: ${postedCands.length}`);
const types = cands.map((c) => (c.includes("typ ") ? c.split("typ ")[1].split(" ")[0] : "unknown"));
console.log("types:", JSON.stringify(types));
// A srflx/relay sighting is positive evidence and stands even if the tail
// of gathering never completed. Blindness only vetoes ABSENCE claims.
const bad = types.filter((t) => t === "srflx" || t === "relay");
if (bad.length) {
  console.log(`SRFLX/RELAY PRESENT: ${JSON.stringify(bad)}`);
  process.exit(1);
}
if (report.blind) blind("page-side deadline hit with no srflx seen");
if (cands.length === 0) blind("complete gathering yielded zero candidates");
console.log("OK: non-vacuous gather with no srflx/relay");
