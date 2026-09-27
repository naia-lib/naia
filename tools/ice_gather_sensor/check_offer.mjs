// Manual sensor: prove the POSTed session offer carries gathered candidates.
//
// Usage:
//   node check_offer.mjs <naia_socket.js> <offer-out> <server-addr>
// Example:
//   node check_offer.mjs ../../socket/client/src/backends/miniquad/naia_socket.js \
//     /tmp/offer.txt http://127.0.0.1:14201/
//
// Prerequisites (see README): live demo server, logging proxy pointed at it,
// headless google-chrome, node >= 22. --disable-web-security is rig-only: the
// page is file:// and XHRs across to the proxy.
//
// Exit codes:
//   0  the POSTed offer is non-vacuous and contains srflx (fixed shape)
//   1  candidate-less offer, offer without srflx, page error, or no offer
//      within the deadline (defect shape, or rig failure -- read the output)
//   2  BLIND -- Chrome missing/unreachable. UNKNOWN, never PASS.
//
// NOT wired into any cargo gate (needs a browser); run by hand when the
// offer path changes.
import { spawn } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, unlinkSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const JS_PATH = resolve(process.argv[2]);
const OFFER_FILE = resolve(process.argv[3]);
const SERVER_ADDR = process.argv[4];
const PAGE_FILE = join(HERE, ".offer_proof.html");
const PORT = 9381;
const DEADLINE_MS = 120000;

function blind(reason) {
  console.log(`BLIND: ${reason}`);
  process.exit(2);
}
if (!SERVER_ADDR) {
  console.log("usage: node check_offer.mjs <naia_socket.js> <offer-out> <server-addr>");
  process.exit(2);
}
if (existsSync(OFFER_FILE)) unlinkSync(OFFER_FILE);
writeFileSync(PAGE_FILE, readFileSync(join(HERE, "offer_proof_template.html"), "utf8")
  .replace("__MQ_JS_PATH__", JS_PATH)
  .replace("__SERVER_ADDR__", SERVER_ADDR));

let chrome;
try {
  chrome = spawn("google-chrome",
    ["--headless=new", "--no-first-run", "--disable-gpu", "--disable-web-security",
     `--remote-debugging-port=${PORT}`, "--remote-allow-origins=*", "about:blank"],
    { stdio: "ignore" });
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
const tab = await (await fetch(
  `http://127.0.0.1:${PORT}/json/new?file://${PAGE_FILE}`, { method: "PUT" })).json();
const ws = new WebSocket(tab.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) pending.get(m.id)(m);
};
const send = (method, params = {}) => new Promise((res) => {
  const mid = ++id;
  pending.set(mid, res);
  ws.send(JSON.stringify({ id: mid, method, params }));
});
await send("Page.enable");
await send("Page.navigate", { url: `file://${PAGE_FILE}` });

let offer = null;
let pageErr = null;
const t0 = Date.now();
while (Date.now() - t0 < DEADLINE_MS) {
  await sleep(1000);
  if (existsSync(OFFER_FILE)) {
    const body = readFileSync(OFFER_FILE, "utf8");
    if (body.length > 0) { offer = body; break; }
  }
  const r = await send("Runtime.evaluate", {
    expression: "document.getElementById('result').textContent", returnByValue: true,
  });
  const text = r?.result?.result?.value ?? "";
  if (text.includes("auth_error")) { pageErr = text; break; }
  if (text.includes("error")) { pageErr = text; break; }
}
ws.close();
kill();

if (pageErr) { console.log("PAGE ERROR:", pageErr); process.exit(1); }
if (!offer) { console.log("NO OFFER POSTED within deadline"); process.exit(1); }
const lines = offer.split(/\r?\n/).filter((l) => l.includes("a=candidate:"));
console.log(`offer candidate lines: ${lines.length}`);
for (const l of lines) console.log("  offer:", l.trim().slice(0, 160));
const types = lines.map((l) => (l.includes("typ ") ? l.split("typ ")[1].split(" ")[0] : "?"));
console.log("types:", JSON.stringify(types));
if (!lines.length) { console.log("CANDIDATELESS OFFER (defect shape)"); process.exit(1); }
if (!types.includes("srflx")) { console.log("NO SRFLX in offer"); process.exit(1); }
console.log("OFFER CARRIES SRFLX");
