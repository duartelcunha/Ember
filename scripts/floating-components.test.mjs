import { test } from "node:test";
import assert from "node:assert/strict";
import { build } from "vite";
import { createServer } from "node:http";
import { mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, extname, dirname, basename } from "node:path";
import puppeteer from "puppeteer";
import { projectRegressions } from "./projects-components.mjs";

// Real React components and CSS, with the documented Tauri IPC mock. This is browser
// evidence only: it cannot establish native focus, input hooks or monitor transitions.
test("UI components preserve geometry and asynchronous ownership", async (t) => {
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
    const port = server.address().port;
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
    page.on("pageerror", error => { errors.push(error.message); console.error("page error", error.message); });
    page.on("console", message => { if (message.type() === "error") console.error("page console", message.text()); });
    await page.setViewport({ width: 640, height: 540, deviceScaleFactor: 1 });
    await page.emulateMediaFeatures([{ name: "prefers-reduced-motion", value: "reduce" }]);
    await page.goto(`http://127.0.0.1:${port}/__ember-test/overlay`);
    await page.waitForFunction(() => document.body.textContent.includes("Snapshot ready"));
    const send = async (name, payload) => page.evaluate((name, payload) => window.__emit(name, payload), name, payload);
    await send("ember://state", { sequence: 99, runId: 2, phase: "error", message: "Obsolete run" });
    await page.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await page.evaluate(() => document.body.textContent.includes("Obsolete run")), false);
    await send("ember://state", { sequence: 100, runId: 4, phase: "hint", message: "A long status message that must wrap and remain readable. ".repeat(8) });
    await page.waitForFunction(() => document.querySelector('.ember-floating')?.textContent.includes('A long status'));
    await presented();
    const bounds = async () => page.$eval('.ember-floating', e => { const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: innerWidth, height: innerHeight }; });
    let rect = await bounds();
    assert.ok(rect.x >= 0 && rect.y >= 0 && rect.right <= rect.width + 1 && rect.bottom <= rect.height + 1, JSON.stringify(rect));
    await page.setViewport({ width: 320, height: 540, deviceScaleFactor: 2 });
    await send("ember://state", { sequence: 101, runId: 4, phase: "refining", project: "VeryLongProjectName".repeat(15), message: "Retrying OpenAI-compatible..." });
    await page.waitForFunction(() => document.querySelector('.ember-orb-row'));
    await presented();
    await capture("project-status");
    const visible = await page.$eval('.ember-orb-row svg', e => { const r = e.getBoundingClientRect(); return { left: r.left + 22, right: r.left + 37, top: r.top + 2, bottom: r.top + 17 }; });
    assert.ok(visible.left >= 0 && visible.right <= 320 && visible.top >= 0 && visible.bottom <= 540, JSON.stringify(visible));
    await capture("project-status");
    await send("ember://state", { sequence: 102, runId: 4, phase: "preview", preview: { original: ['First line\nSecond line'], result: ['Changed line\nSecond line'], page: 0 } });
    await page.setViewport({ width: 320, height: 540, deviceScaleFactor: 2 });
    await page.waitForFunction(() => document.body.textContent.includes('Review changes'));
    await presented();
    rect = await bounds();
    assert.ok(rect.right <= rect.width + 1 && rect.bottom <= rect.height + 1, JSON.stringify(rect));
    const preview = await page.$eval('.ember-preview', e => ({ height: e.getBoundingClientRect().height, max: innerHeight * .4, background: getComputedStyle(e).backgroundColor }));
    assert.ok(preview.height <= preview.max + 1, JSON.stringify(preview));
    assert.equal(preview.background, 'rgb(26, 21, 17)');
    await capture("preview");
    await page.setViewport({ width: 320, height: 480, deviceScaleFactor: 2 });
    await send('ember://state', { sequence: 103, runId: 4, phase: 'preview', preview: { original: ['W'.repeat(48)], result: ['界'.repeat(24)], page: 0 } });
    await presented();
    const complete = await page.$eval('.ember-preview', e => ({ card: e.getBoundingClientRect().bottom, footer: e.lastElementChild.getBoundingClientRect().bottom, height: e.scrollHeight, max: innerHeight * .4 }));
    assert.ok(complete.footer <= complete.card && complete.height <= complete.max, JSON.stringify(complete));
    await page.setViewport({ width: 800, height: 600, deviceScaleFactor: 1 });
    await send('ember://overlay-at', { sequence: 1000, generation: 2, ready: true, scale: 1, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
    await send('ember://state', { sequence: 104, runId: 4, phase: 'refining', project: 'Ember' });
    await page.waitForSelector('.ember-orb-row svg');
    await presented();
    const ink = await page.$eval('.ember-orb-row svg', e => { const r = e.getBoundingClientRect(); return { x: r.x + 22, y: r.y + 2 }; });
    assert.equal(ink.x, 310); assert.equal(ink.y, 182);
    await send('ember://overlay-at', { sequence: 1001, generation: 3, ready: false, scale: 2, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
    await presented();
    assert.equal(await page.$eval('.ember-floating', e => getComputedStyle(e).visibility), 'hidden');
    await send('ember://overlay-at', { sequence: 1002, generation: 4, ready: true, scale: 1, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
    await presented();
    assert.equal(await page.$eval('.ember-floating', e => getComputedStyle(e).visibility), 'visible');
    await capture('orb-anchor');
    await page.goto(`http://127.0.0.1:${port}/__ember-test/picker`);
    await page.waitForFunction(() => window.__pickerReady === true && document.querySelector('.ember-floating'));
    await send('ember://picker', { sequence: 20, rows: Array.from({ length: 20 }, (_, i) => ({ id: `${i}`, name: `Project ${i}`, color: '#fd8c3c', icon: 'sparkle' })), index: 19, open: true, chosen: null });
    await page.waitForSelector('[role=option][aria-selected=true]');
    assert.equal(await page.$eval('[role=option][aria-selected=true]', e => e.textContent.trim()), 'Project 19');
    await send('ember://picker', { sequence: 19, rows: [], index: 0, open: false, chosen: null });
    await page.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await page.$$eval('[role=option][aria-selected=true]', e => e.length), 1);
    await capture("picker");
    await t.test("profile imports require review and discard obsolete responses", async () => {
    await page.goto(`http://127.0.0.1:${port}/__ember-test/profile`);
    await page.waitForSelector('textarea');
    const clickButton = async label => page.evaluate(label => {
      const button = Array.from(document.querySelectorAll('button')).find(button => button.textContent === label);
      if (!button || button.disabled) throw new Error(`Button unavailable: ${label}`);
      button.click();
    }, label);
    const enterProfile = async text => {
      await page.focus('textarea');
      await page.$eval('textarea', element => element.select());
      await page.keyboard.type(text);
    };
    const resolveImport = draft => page.evaluate(draft => window.__profileFixture.resolveImport(draft), draft);
    await clickButton('Import files...');
    await page.waitForFunction(() => window.__profileFixture.imports === 1);
    await enterProfile('Tone: my newer edit');
    await resolveImport({ text: 'Tone: stale import', sources: [], warnings: ['Old import'] });
    await page.waitForFunction(() => Array.from(document.querySelectorAll('button')).some(button => button.textContent === 'Import files...' && !button.disabled));
    assert.equal(await page.$eval('textarea', element => element.value), 'Tone: my newer edit');
    assert.equal(await page.evaluate(() => window.__profileFixture.saved.length), 0);

    await clickButton('Import files...');
    await page.waitForFunction(() => window.__profileFixture.imports === 2);
    await clickButton('Use Ember default');
    await page.waitForFunction(() => document.querySelector('textarea').value === 'Tone: default');
    await resolveImport({ text: 'Tone: stale after reset', sources: [], warnings: [] });
    await presented();
    assert.equal(await page.$eval('textarea', element => element.value), 'Tone: default');

    await page.waitForFunction(() => Array.from(document.querySelectorAll('button')).some(button => button.textContent === 'Import files...' && !button.disabled));
    await clickButton('Import files...');
    await page.waitForFunction(() => window.__profileFixture.imports === 3);
    await resolveImport({ text: 'Tone: reviewed', sources: [{ path: '/fixture/AGENTS.md', fingerprint: 'a'.repeat(64), bytes: 80 }], warnings: ['Operational lines were excluded.'] });
    await page.waitForFunction(() => document.body.textContent.includes('Operational lines were excluded.'));
    assert.equal(await page.evaluate(() => window.__profileFixture.saved.length), 0);
    await clickButton('Save reviewed profile');
    await page.waitForFunction(() => window.__profileFixture.saved.length === 1);
    const savedProfile = await page.evaluate(() => window.__profileFixture.saved[0]);
    assert.equal(savedProfile.text, 'Tone: reviewed');
    assert.equal(savedProfile.sources[0].fingerprint, 'a'.repeat(64));
    await page.waitForFunction(() => !document.querySelector('textarea').disabled);
    await enterProfile('é'.repeat(4097));
    await page.waitForSelector('[role=alert]');
    assert.equal(await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(button => button.textContent === 'Save reviewed profile').disabled), true);
    assert.equal(await page.evaluate(() => window.__profileFixture.saved.length), 1);
    await capture('profile-review');
    });
    await t.test("profile migration preserves the original and requires an explicit save", async () => {
      await page.goto(`http://127.0.0.1:${port}/__ember-test/profile-migration`);
      await page.waitForFunction(() => document.body.textContent.includes('Review imported instructions'));
      await page.evaluate(() => Array.from(document.querySelectorAll('summary')).find(e => e.textContent === 'Review imported instructions').click());
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent === 'Use as draft').click());
      assert.equal(await page.$eval('textarea', e => e.value.includes('Run commands')), false);
      assert.equal(await page.evaluate(() => window.__profileFixture.saved.length), 0);
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent === 'Save reviewed profile').click());
      await page.waitForFunction(() => window.__profileFixture.saved.length === 1 && document.body.textContent.includes('Previous profile'));
      assert.equal(await page.evaluate(() => Array.from(document.querySelectorAll('details')).find(e => e.textContent.startsWith('Previous profile')).textContent.includes('Run commands')), true);
    });
    await t.test("context inspector stays concise and rejects obsolete snapshots", async () => {
      await page.setViewport({ width: 640, height: 540, deviceScaleFactor: 1 });
      await page.goto(`http://127.0.0.1:${port}/__ember-test/context`);
      await page.waitForFunction(() => window.__contextFixture.pending.length === 1);
      const snapshot = { runId: 11, project: "Ember", selection: "auto", reason: "Project path", profile: "Tone: direct", profileSources: [], profileReviewNeeded: false, profileInvalid: false, projectContext: "Stack: Rust", sources: [], sourceStatus: "Up to date", delivery: "prepared" };
      await page.evaluate(value => window.__contextFixture.pending.shift()(value), snapshot);
      await page.waitForFunction(() => document.body.textContent.includes('Ember · Automatic'));
      assert.equal(await page.$$eval('details[open]', e => e.length), 0);
      await capture('context-collapsed');
      await page.click('summary');
      assert.equal(await page.$eval('section', e => e.innerText.includes('Prepared')), true);
      await page.click('button');
      await page.waitForFunction(() => window.__contextFixture.pending.length === 1);
      await page.evaluate(value => window.__contextFixture.pending.shift()(value), { ...snapshot, runId: 10, project: 'Obsolete', delivery: 'sent' });
      await presented();
      assert.equal(await page.$eval('section', e => e.innerText.includes('Obsolete')), false);
      await page.click('button');
      await page.waitForFunction(() => window.__contextFixture.pending.length === 1);
      await page.evaluate(value => window.__contextFixture.pending.shift()(value), { ...snapshot, delivery: 'sent' });
      await page.waitForFunction(() => document.querySelector('section').innerText.includes('Sent'));
      await capture('context-expanded');
    });
    await t.test("project distillation and colour responses preserve the current draft", () => projectRegressions(page, `http://127.0.0.1:${port}`));
    assert.deepEqual(errors, []);
  } finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); assert.equal(dirname(directory), resolve(tmpdir())); assert.ok(basename(directory).startsWith("ember-browser-test-")); await rm(directory, { recursive: true, force: true }); }
});
