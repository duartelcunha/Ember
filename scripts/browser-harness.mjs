import assert from "node:assert/strict";
import { build } from "vite";
import { createServer } from "node:http";
import { mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, extname, dirname, basename } from "node:path";
import puppeteer from "puppeteer";

/**
 * Builds the fixture bundle once, serves it, opens a headless page and hands it to `fn`.
 *
 * Real React components and CSS, with the documented Tauri IPC mock. This is browser evidence
 * only: it cannot establish native focus, input hooks or monitor transitions.
 *
 * Shared by the overlay test and the settings test so that a regression in one surface does
 * not abort the other's assertions: they used to run as one sequential test, and an overlay
 * failure hid every settings check that came after it.
 */
export async function withBrowser(fn) {
  const directory = await mkdtemp(join(tmpdir(), "ember-browser-test-"));
  await build({ logLevel: "error", build: { outDir: directory, emptyOutDir: false,
    rollupOptions: { input: resolve("scripts/floating-fixture.html") } } });
  const server = createServer(async (req, res) => {
    try {
      const pathname = new URL(req.url, "http://localhost").pathname;
      const relative = pathname.startsWith("/assets/") ? pathname.slice(1) : "scripts/floating-fixture.html";
      if (relative.includes("..")) { res.writeHead(400).end(); return; }
      const mime = { ".js": "text/javascript", ".css": "text/css", ".html": "text/html", ".woff2": "font/woff2" };
      res.setHeader("Content-Type", mime[extname(relative)] ?? "application/octet-stream");
      res.end(await readFile(join(directory, relative)));
    } catch { res.writeHead(404).end(); }
  });
  let browser;
  try {
    await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
    const origin = `http://127.0.0.1:${server.address().port}`;
    browser = await puppeteer.launch({ headless: true,
      // Ubuntu AppArmor authorizes the system Chrome sandbox, not downloaded CfT.
      ...(process.env.CI && process.platform === "linux" ? { channel: "chrome" } : {}),
    });
    const page = await browser.newPage();
    await page.bringToFront();
    const errors = [];
    const presented = () => page.evaluate(() => new Promise(resolve =>
      requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const capture = async (name) => {
      if (!process.env.EMBER_TEST_CAPTURE_DIR) return;
      await mkdir(process.env.EMBER_TEST_CAPTURE_DIR, { recursive: true });
      await page.screenshot({ path: join(process.env.EMBER_TEST_CAPTURE_DIR, name + ".png") });
    };
    const send = async (name, payload) => page.evaluate((name, payload) => window.__emit(name, payload), name, payload);
    page.on("pageerror", error => { errors.push(error.message); console.error("page error", error.message); });
    page.on("console", message => { if (message.type() === "error") console.error("page console", message.text()); });
    await page.emulateMediaFeatures([{ name: "prefers-reduced-motion", value: "reduce" }]);
    await fn({ page, origin, capture, presented, send, errors });
    assert.deepEqual(errors, []);
  } finally {
    await browser?.close();
    await new Promise(resolve => server.close(resolve));
    assert.equal(dirname(directory), resolve(tmpdir()));
    assert.ok(basename(directory).startsWith("ember-browser-test-"));
    await rm(directory, { recursive: true, force: true });
  }
}
