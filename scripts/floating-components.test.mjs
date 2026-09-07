import { test } from "node:test";
import assert from "node:assert/strict";

const hexToRgb = (hex) => {
  const [r, g, b] = hex.replace("#", "").match(/../g).map((part) => parseInt(part, 16));
  return `rgb(${r}, ${g}, ${b})`;
};
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { withBrowser } from "./browser-harness.mjs";

// The anchor numbers below are derived from the source constants, never pinned. Three separate
// design decisions moved them (the gap, centre anchoring, the shared chip spec) and each one
// left this suite red about arithmetic instead of about behaviour.
const geometrySource = await readFile(new URL("../src/components/floatingGeometry.ts", import.meta.url), "utf8");
const { CURSOR_GAP, ORB_INK } = await import(`data:text/javascript;base64,${Buffer.from(
  ts.transpileModule(geometrySource, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText,
).toString("base64")}`);

// Overlay, picker and splash. The settings surfaces have their own test file so that a
// regression here does not hide theirs.
test("UI components preserve geometry and asynchronous ownership", (t) => withBrowser(async ({ page, origin, capture, presented, send }) => {
    await page.setViewport({ width: 640, height: 540, deviceScaleFactor: 1 });
    await page.goto(`${origin}/__ember-test/overlay`);
    await page.waitForFunction(() => document.body.textContent.includes("Snapshot ready"));
    await t.test('hint messages do not draw a second pointer', async () => {
      assert.equal(await page.$$eval('.ember-bubble svg', nodes => nodes.length), 0);
    });
    await send("ember://state", { sequence: 99, runId: 2, phase: "error", message: "Obsolete run" });
    await page.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await page.evaluate(() => document.body.textContent.includes("Obsolete run")), false);
    await send("ember://state", { sequence: 100, runId: 4, phase: "hint", message: "A long status message that must wrap and remain readable. ".repeat(8) });
    await page.waitForFunction(() => document.querySelector('.ember-floating')?.textContent.includes('A long status'));
    await presented();
    const bounds = async () => page.$eval('.ember-floating', e => { const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, centre: (r.top + r.bottom) / 2, width: innerWidth, height: innerHeight }; });
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
    await page.evaluate(() => {
      window.__phaseOverlaps = [];
      window.__phaseObserver = new MutationObserver(() => {
        window.__phaseOverlaps.push(Boolean(document.querySelector('.ember-orb-row') && document.querySelector('.ember-confirmation')));
      });
      window.__phaseObserver.observe(document.getElementById('root'), { childList: true, subtree: true });
    });
    await send("ember://state", { sequence: 102, runId: 4, phase: "preview", confirmationScope: 'selection' });
    await page.setViewport({ width: 320, height: 540, deviceScaleFactor: 2 });
    await page.waitForFunction(() => document.querySelector('.ember-confirmation'));
    await presented();
    await t.test('review replaces the orb without retaining an exiting loading layer', async () => {
      const overlaps = await page.evaluate(() => { window.__phaseObserver.disconnect(); return window.__phaseOverlaps; });
      assert.ok(overlaps.length > 0);
      assert.equal(overlaps.some(Boolean), false);
      assert.equal(await page.$$eval('.ember-orb-row', nodes => nodes.length), 0);
    });
    rect = await bounds();
    assert.ok(rect.right <= rect.width + 1 && rect.bottom <= rect.height + 1, JSON.stringify(rect));
    const confirmation = await page.$eval('.ember-confirmation', e => ({ height: e.getBoundingClientRect().height, background: getComputedStyle(e).backgroundColor, text: e.textContent, token: getComputedStyle(document.documentElement).getPropertyValue('--color-surface-1').trim() }));
    // 30 is the chip spec resolved: 16px line-height plus 6px padding top and bottom is the
    // 28px min-height, and `.ember-bubble` adds a 1px border on each side. It is pinned exactly
    // so the confirmation cannot quietly grow into a document viewer again.
    assert.equal(confirmation.height, 30);
    // The surface reads from the token rather than a literal, so this compares the two instead
    // of pinning a hex that moves whenever the palette does.
    assert.equal(confirmation.background, hexToRgb(confirmation.token));
    assert.equal(confirmation.text, 'Enter apply · Esc cancel');
    await capture("confirmation");
    await t.test('whole-field confirmation exposes scope without document content', async () => {
      await page.setViewport({ width: 170, height: 480, deviceScaleFactor: 2 });
      await send('ember://state', { sequence: 103, runId: 4, phase: 'preview', confirmationScope: 'field', preview: { original: ['PRIVATE ORIGINAL'], result: ['PRIVATE RESULT'], page: 0 } });
      await presented();
      const compact = await page.$eval('.ember-confirmation', e => ({ width: e.getBoundingClientRect().width, height: e.getBoundingClientRect().height, text: e.textContent }));
      // Two lines of the shared chip spec: 16px line-height twice, 12px padding, 2px border.
      assert.ok(compact.width <= 154 && compact.height <= 48, JSON.stringify(compact));
      assert.equal(compact.text, 'Whole field · Enter apply · Esc cancel');
      assert.equal(await page.evaluate(() => document.body.textContent.includes('PRIVATE')), false);
    });
    await page.setViewport({ width: 800, height: 600, deviceScaleFactor: 1 });
    await send('ember://overlay-at', { sequence: 1000, generation: 2, ready: true, scale: 1, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
    await send('ember://state', { sequence: 104, runId: 4, phase: 'refining', project: 'Ember' });
    await page.waitForSelector('.ember-orb-row svg');
    await presented();
    const ink = await page.$eval('.ember-orb-row svg', e => { const r = e.getBoundingClientRect(); return { x: r.x + 22, y: r.y + 2 }; });
    assert.equal(ink.x, 300 + CURSOR_GAP.x);
    // Rounded because the controller snaps the cursor-facing edge to a whole device pixel:
    // centring a 15px ring lands on a half pixel, and a half-lit row of pixel art is exactly
    // what that snap exists to prevent.
    assert.equal(ink.y, Math.round(180 + CURSOR_GAP.y - ORB_INK.height / 2));
    await send('ember://overlay-at', { sequence: 1001, generation: 3, ready: false, scale: 2, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
    await presented();
    assert.equal(await page.$eval('.ember-floating', e => getComputedStyle(e).visibility), 'hidden');
    await send('ember://overlay-at', { sequence: 1002, generation: 4, ready: true, scale: 1, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
    await presented();
    assert.equal(await page.$eval('.ember-floating', e => getComputedStyle(e).visibility), 'visible');
    await capture('orb-anchor');
    await t.test('acceptance keeps the cursor-facing edge when review becomes a pill or orb', async () => {
      let sequence = 110;
      for (const scale of [1, 1.25, 1.5, 1.75, 2]) {
        const near = (actual, expected) => assert.ok(Math.abs(actual - expected) <= .51 / scale, `${actual} is not within half a physical pixel of ${expected} at ${scale}`);
        await page.setViewport({ width: 800, height: 600, deviceScaleFactor: scale });
        await send('ember://overlay-at', { sequence: 2000 + sequence, generation: sequence, ready: true, scale, width: 800 * scale, height: 600 * scale, x: -1000 + 300 * scale, y: -500 + 180 * scale, originX: -1000, originY: -500 });
        await send('ember://state', { sequence: sequence++, runId: 5, phase: 'preview', confirmationScope: 'selection' });
        await presented();
        let card = await bounds();
        near(card.x, 300 + CURSOR_GAP.x);
        // The surface is anchored by its centre, so this holds whatever its height turns out
        // to be. Pinning the top edge only worked while every surface was one line tall.
        near(card.centre, 180 + CURSOR_GAP.y);
        await send('ember://overlay-at', { sequence: 2000 + sequence, generation: sequence, ready: true, scale, width: 800 * scale, height: 600 * scale, x: -1000 + 710 * scale, y: -500 + 180 * scale, originX: -1000, originY: -500 });
        await presented();
        card = await bounds();
        near(card.right, 710 - CURSOR_GAP.x);
        // This is the state emitted after native Enter acceptance and application.
        // The browser test does not replace qualification of the native input hook.
        await send('ember://state', { sequence: sequence++, runId: 5, phase: 'success', message: 'Sent' });
        await presented();
        card = await bounds();
        near(card.right, 710 - CURSOR_GAP.x); near(card.centre, 180 + CURSOR_GAP.y);
        await send('ember://state', { sequence: sequence++, runId: 5, phase: 'refining' });
        await presented();
        const ring = await page.$eval('.ember-orb-row svg', e => { const r = e.getBoundingClientRect(); return { right: r.x + 37, y: r.y + 2 }; });
        near(ring.right, 710 - CURSOR_GAP.x);
        near(ring.y, 180 + CURSOR_GAP.y - ORB_INK.height / 2);
      }
    });
    await t.test('signature motion and surface morph preserve the anchor without retaining the orb', async () => {
      await page.setViewport({ width: 800, height: 600, deviceScaleFactor: 1 });
      await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
      let sequence = 300;
      // A hint fired with nothing selected goes straight from hidden to hint, so it never gets
      // the morph. It used to arrive fully drawn in a single frame; it now opens from the
      // cursor-facing edge like every other surface.
      await send('ember://overlay-at', { sequence: 4999, generation: 299, ready: true, scale: 1, width: 800, height: 600, x: 300, y: 180, originX: 0, originY: 0 });
      await send('ember://state', { sequence: 299, runId: 6, phase: 'hidden' });
      await presented();
      await send('ember://state', { sequence: 300, runId: 6, phase: 'hint', message: 'Select text first' });
      await presented();
      assert.equal(await page.$eval('[data-enter]', e => getComputedStyle(e).animationName), 'ember-surface-open');
      assert.equal(await page.$$eval('[data-morph-from-orb]', nodes => nodes.length), 0);
      sequence = 301;
      for (const x of [300, 790]) {
        await send('ember://overlay-at', { sequence: 5000 + sequence, generation: sequence, ready: true, scale: 1, width: 800, height: 600, x, y: 180, originX: 0, originY: 0 });
        await send('ember://state', { sequence: sequence++, runId: 6, phase: 'refining' });
        await presented();
        assert.equal(await page.$$eval('.ember-orb-row circle', nodes => nodes.filter(e => getComputedStyle(e).animationName === 'ember-chase').length), 8);
        assert.equal(await page.$$eval('.ember-orb-row animate', nodes => nodes.length), 1);
        await page.evaluate(() => {
          window.__morphAnimation = null;
          document.getElementById('root').addEventListener('animationstart', function started(event) {
            if (event.animationName !== 'ember-surface-morph') return;
            this.removeEventListener('animationstart', started);
            window.__morphAnimation = event.target.getAnimations().find(a => a.animationName === event.animationName);
            window.__morphAnimation.pause();
            window.__morphAnimation.currentTime = 0;
          });
        });
        await send('ember://state', { sequence: sequence++, runId: 6, phase: 'preview', confirmationScope: 'selection' });
        await page.waitForFunction(() => window.__morphAnimation !== null);
        const start = await bounds();
        assert.equal(await page.$$eval('.ember-orb-row', nodes => nodes.length), 0);
        const morph = await page.$eval('[data-morph-from-orb]', e => {
          const style = getComputedStyle(e);
          const r = e.getBoundingClientRect();
          return { clip: style.clipPath, opacity: style.opacity, transform: style.transform, side: e.parentElement.dataset.side, width: r.width, height: r.height };
        });
        assert.equal(morph.opacity, '1');
        assert.equal(morph.transform, 'none');
        assert.equal(morph.side, x === 300 ? 'right' : 'left');
        // The morph must begin where the ring is: a 15px band centred on the cursor-facing
        // edge. Chromium leaves calc() unresolved in the computed value, so this pins the two
        // properties that were wrong rather than parsing arithmetic. Substring-matching only
        // '15px' passed happily while the start was pinned to the top corner, 7.5px above the
        // ring, which is exactly the bug this now catches.
        assert.ok(morph.clip.includes('15px'), morph.clip);
        assert.ok(morph.clip.startsWith('inset(calc(50%'), morph.clip);
        // Flush to the edge the cursor is on: the last inset is zero on the right, the second
        // is zero on the left.
        assert.ok((x === 300 ? / 0px round / : /px\) 0px calc/).test(morph.clip), morph.clip);
        await page.evaluate(() => { window.__morphAnimation.currentTime = 90; });
        await presented();
        assert.deepEqual(await bounds(), start);
        await capture(x === 300 ? 'morph-right' : 'morph-left');
        // A new state interrupts the morph without an old layer or a late callback.
        await send('ember://state', { sequence: sequence++, runId: 6, phase: 'success', message: 'Sent' });
        await presented();
        assert.equal(await page.$$eval('[data-morph-from-orb], .ember-confirmation, .ember-orb-row', nodes => nodes.length), 0);
      }
      // Closing is the same gesture backwards, on the surface that is already there. It arrives
      // on the SAME phase on purpose: a different phase would remount the wrapper and the pill
      // would be replaced rather than collapse. The box must not move while it does, for the
      // same reason the morph must not: the cursor anchor is measured from it.
      const settled = await bounds();
      await send('ember://state', { sequence: sequence++, runId: 6, phase: 'success', message: 'Sent', closing: true });
      await presented();
      const leaving = await page.$eval('[data-leave]', e => {
        const style = getComputedStyle(e);
        return { name: style.animationName, transform: style.transform, fill: style.animationFillMode };
      });
      assert.equal(leaving.name, 'ember-surface-close');
      assert.equal(leaving.transform, 'none');
      // Without `forwards` the surface snaps back to full size for the frames between the end of
      // the animation and the native hide.
      assert.equal(leaving.fill, 'forwards');
      assert.deepEqual(await bounds(), settled);
      await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
      await send('ember://state', { sequence: sequence++, runId: 6, phase: 'refining' });
      await presented();
      assert.equal(await page.$$eval('.ember-orb-row animate', nodes => nodes.length), 0);
      assert.equal(await page.$$eval('.ember-orb-row circle', nodes => nodes.every(e => getComputedStyle(e).animationName === 'none')), true);
      await send('ember://state', { sequence: sequence++, runId: 6, phase: 'preview', confirmationScope: 'selection' });
      await presented();
      assert.equal(await page.$eval('[data-morph-from-orb]', e => getComputedStyle(e).animationName), 'none');
      await send('ember://state', { sequence: sequence++, runId: 6, phase: 'hidden' });
      await presented();
      await send('ember://state', { sequence: sequence++, runId: 6, phase: 'hint', message: 'Select text first' });
      await presented();
      assert.equal(await page.$eval('[data-enter]', e => getComputedStyle(e).animationName), 'none');
      assert.equal(await page.$$eval('.ember-chip > *', nodes => nodes.every(e => getComputedStyle(e).animationName === 'none')), true);
      await send('ember://state', { sequence: sequence++, runId: 6, phase: 'hint', message: 'Select text first', closing: true });
      await presented();
      assert.equal(await page.$eval('[data-leave]', e => getComputedStyle(e).animationName), 'none');
    });
    await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
    await page.goto(`${origin}/__ember-test/picker`);
    await page.waitForFunction(() => window.__pickerReady === true && document.querySelector('.ember-floating'));
    await send('ember://picker', { sequence: 20, rows: Array.from({ length: 20 }, (_, i) => ({ id: `${i}`, name: `Project ${i}`, color: '#fd8c3c', icon: 'sparkle' })), index: 19, open: true, chosen: null });
    await page.waitForSelector('[role=option][aria-selected=true]');
    assert.equal(await page.$eval('[role=option][aria-selected=true]', e => e.textContent.trim()), 'Project 19');
    // The list opens like every other surface anchored to the cursor, and it TRAVELS: all twenty
    // rows stay mounted and the column slides, instead of a nine-row slice being recut under a
    // selection pill that was the only thing animating.
    assert.equal(await page.$eval('.ember-bubble[data-enter]', e => getComputedStyle(e).animationName), 'ember-surface-open');
    assert.equal(await page.$$eval('[role=option]', nodes => nodes.length), 20);
    const framed = await page.evaluate(() => {
      const row = document.querySelector('[role=option][aria-selected=true]').getBoundingClientRect();
      const window_ = document.querySelector('[data-rows]').getBoundingClientRect();
      return { above: row.top - window_.top, below: window_.bottom - row.bottom };
    });
    assert.ok(framed.above >= -1 && framed.below >= -1, JSON.stringify(framed));
    await send('ember://picker', { sequence: 19, rows: [], index: 0, open: false, chosen: null });
    await page.evaluate(() => new Promise(requestAnimationFrame));
    assert.equal(await page.$$eval('[role=option][aria-selected=true]', e => e.length), 1);
    await capture("picker");
    await t.test('the tray menu opens out of the icon, answers the keyboard and folds back', async () => {
      // The first click can beat the page: the mount-time `ready` handshake has to open it.
      await page.goto(`${origin}/__ember-test/tray?trayOpen`);
      await page.waitForSelector('[role=menu]');
      assert.equal(await page.$$eval('[role=menuitem]', nodes => nodes.length), 2);
      assert.equal(await page.$eval('[role=menu]', e => e.textContent.trim()), 'EmberSettingsQuit Ember');
      // Born from the icon below it: the house entrance, aimed at the bottom edge, and the menu
      // node itself holds focus so the keyboard and a screen reader have a composite to follow.
      assert.equal(await page.$eval('.ember-bubble[data-enter]', e => getComputedStyle(e).animationName), 'ember-surface-open');
      assert.equal(await page.$eval('.ember-tray', e => e.dataset.side), 'above');
      assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('role')), 'menu');
      const active = () => page.$eval('[role=menu]', e => document.getElementById(e.getAttribute('aria-activedescendant'))?.querySelector('span')?.textContent.trim());
      assert.equal(await active(), 'Settings');
      await page.keyboard.press('ArrowDown');
      await page.waitForFunction(() => document.querySelector('[role=menuitem][data-active]')?.textContent.startsWith('Quit'));
      assert.equal(await active(), 'Quit Ember');
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => window.__trayActions.at(-1) === 'close');
      // Rust answers a close by flipping `open`; the surface folds before the window hides, and
      // nothing chosen during the fold reaches Rust. The fold is 140ms, shorter than a slow
      // runner's round trip, so the proof is the animation START seen from inside the page.
      const folding = page.evaluate(() => new Promise((resolve) => {
        document.addEventListener('animationstart', (event) => {
          if (event.animationName === 'ember-surface-close') resolve(event.target.hasAttribute('data-leave'));
        }, true);
        setTimeout(() => resolve('never started'), 2000);
      }));
      await send('ember://tray', { open: false });
      assert.equal(await folding, true);
      await page.keyboard.press('Enter');
      await page.waitForFunction(() => !document.querySelector('[role=menu]'));
      assert.equal(await page.evaluate(() => window.__trayActions.at(-1)), 'close');
      // A taskbar at the top puts the menu below the icon: it opens out of its top edge instead.
      // And Enter chooses once, however long it is held.
      await send('ember://tray', { open: true, below: true });
      await page.waitForSelector('[role=menu]');
      assert.equal(await page.$eval('.ember-tray', e => e.dataset.side), 'below');
      assert.ok((await page.$eval('.ember-bubble[data-enter]', e => getComputedStyle(e).getPropertyValue('--ember-open-start'))).includes('0 0 60%'));
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      await page.keyboard.press('Enter');
      await page.waitForFunction(() => window.__trayActions.at(-1) === 'quit');
      assert.equal(await page.evaluate(() => window.__trayActions.filter(a => a === 'quit').length), 1);
      await capture('tray');
    });
    await t.test('static startup branding still completes its native lifecycle', async () => {
      // Declared here rather than inherited from whatever ran before: this asserts the REDUCED
      // branding, and it read as an intermittent failure whenever an earlier block left the
      // preference at no-preference.
      await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
      await page.goto(`${origin}/__ember-test/splash?mode=startup`);
      await page.waitForFunction(() => window.__closed === 'close_splash');
      assert.equal(await page.$eval('img', e => getComputedStyle(e).transform), 'none');
      assert.equal(await page.$eval('img', e => getComputedStyle(e).opacity), '1');
    });
}));
