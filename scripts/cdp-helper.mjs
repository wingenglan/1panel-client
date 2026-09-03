// Temporary CDP driver for WebView2 (deleted after use). No credentials in this file.
import { writeFileSync } from "node:fs";

const cmd = process.argv[2];

async function getPageWs() {
  const res = await fetch("http://127.0.0.1:9222/json/list");
  const targets = await res.json();
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error("no page target");
  return page.webSocketDebuggerUrl;
}

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
      }
    };
  }
  send(method, params = {}) {
    const id = ++this.id;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
}

const ws = new WebSocket(await getPageWs());
await new Promise((r) => (ws.onopen = r));
const cdp = new Cdp(ws);
await cdp.send("Runtime.enable");
await cdp.send("Page.enable");

if (cmd === "eval") {
  const expr = process.argv[3];
  const r = await cdp.send("Runtime.evaluate", {
    expression: expr,
    returnByValue: true,
    awaitPromise: true,
  });
  if (r.exceptionDetails) {
    console.error("EXCEPTION:", JSON.stringify(r.exceptionDetails, null, 2));
    process.exit(2);
  }
  console.log(JSON.stringify(r.result?.value ?? r, null, 2));
} else if (cmd === "shot") {
  const file = process.argv[3];
  const r = await cdp.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(file, Buffer.from(r.data, "base64"));
  console.log("saved", file);
} else if (cmd === "clickAt") {
  const x = Number(process.argv[3]);
  const y = Number(process.argv[4]);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
  console.log("clicked", x, y);
} else if (cmd === "hoverAt") {
  const x = Number(process.argv[3]);
  const y = Number(process.argv[4]);
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  console.log("hovered", x, y);
} else if (cmd === "type") {
  await cdp.send("Input.insertText", { text: process.argv[3] });
  console.log("typed", process.argv[3]);
} else if (cmd === "key") {
  await cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: process.argv[3], code: process.argv[3] === "Escape" ? "Escape" : "Enter", windowsVirtualKeyCode: process.argv[3] === "Escape" ? 27 : 13, nativeVirtualKeyCode: process.argv[3] === "Escape" ? 27 : 13 });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: process.argv[3], code: process.argv[3] === "Escape" ? "Escape" : "Enter", windowsVirtualKeyCode: process.argv[3] === "Escape" ? 27 : 13, nativeVirtualKeyCode: process.argv[3] === "Escape" ? 27 : 13 });
  console.log("key", process.argv[3]);
}
process.exit(0);
