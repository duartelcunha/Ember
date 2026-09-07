import { test } from "node:test";
import assert from "node:assert/strict";
import { withBrowser } from "./browser-harness.mjs";
import { projectRegressions } from "./projects-components.mjs";
import { appearanceRegressions, openTab, settingsLayoutRegressions, settingsRegressions, tabContentRegressions } from "./settings-components.mjs";

// Every settings surface: the profile editor, the context line, the projects tab and the full
// settings shell. Browser evidence only, like the overlay test.
test("settings surfaces fit the window and keep their behaviour", (t) => withBrowser(async ({ page, origin, capture, presented }) => {
    await t.test("profile imports require review and discard obsolete responses", async () => {
    await page.goto(`${origin}/__ember-test/profile`);
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
      await page.goto(`${origin}/__ember-test/profile-migration`);
      await page.waitForFunction(() => document.body.textContent.includes('Review imported instructions'));
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent.startsWith('Review imported instructions')).click());
      await page.waitForSelector('[role=dialog]');
      assert.equal(await page.$eval('[role=dialog]', e => e.textContent.includes('Run commands')), true);
      await capture('dialog-profile-review');
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent === 'Use as draft').click());
      await page.waitForFunction(() => !document.querySelector('[role=dialog]'));
      assert.equal(await page.$eval('textarea', e => e.value.includes('Run commands')), false);
      assert.equal(await page.evaluate(() => window.__profileFixture.saved.length), 0);
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent === 'Save reviewed profile').click());
      await page.waitForFunction(() => window.__profileFixture.saved.length === 1 && Array.from(document.querySelectorAll('button')).some(e => e.textContent.startsWith('Restore previous')));
      assert.equal(await page.$$eval('details', e => e.length), 0);
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent.startsWith('Restore previous')).click());
      await page.waitForSelector('[role=dialog] pre');
      assert.equal(await page.$eval('[role=dialog] pre', e => e.textContent.includes('Run commands')), true);
      await capture('dialog-profile-restore');
      await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent === 'Restore as draft').click());
      await page.waitForFunction(() => document.querySelector('textarea').value.includes('Run commands'));
      // Restoring is a draft, never a save.
      assert.equal(await page.evaluate(() => window.__profileFixture.saved.length), 1);
    });
    await t.test("layers leave the way they arrived", async () => {
      // Every page in this harness runs with reduced motion, which is also the state in which a
      // missing exit is invisible. Radix only defers the unmount while an animation is declared
      // for the closed state, so this is what proves the exit exists at all: before it, every
      // dialog, popover and scrim in the settings vanished between two frames.
      await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
      const open = async () => {
        await page.goto(`${origin}/__ember-test/profile-migration`);
        await page.waitForFunction(() => document.body.textContent.includes('Review imported instructions'));
        await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent.startsWith('Review imported instructions')).click());
        await page.waitForSelector('[role=dialog]');
      };
      await open();
      assert.equal(await page.$eval('.ember-dialog', e => getComputedStyle(e).animationName), 'ember-layer-in');
      // The contract, read from inside the page the moment the state flips: Radix keeps the
      // layer mounted with data-state=closed, and the cascade gives that state the exit
      // animation. Polled with setTimeout rather than proven by `animationstart`, because a
      // headless macOS runner did not deliver that event inside a second while the CSS was
      // right; and not read on the next round trip, because the exit is 140ms and a slow runner
      // can unmount before the reply lands.
      const leaving = await page.evaluate(() => new Promise((resolve) => {
        Array.from(document.querySelectorAll('button')).find(e => e.textContent === 'Use as draft').click();
        const started = performance.now();
        const poll = () => {
          const layer = document.querySelector('.ember-dialog');
          const scrim = document.querySelector('.ember-dialog-overlay');
          if (!layer) return resolve('unmounted before a closed state was seen');
          if (layer.dataset.state === 'closed') {
            return resolve({ layer: getComputedStyle(layer).animationName, scrim: scrim && getComputedStyle(scrim).animationName });
          }
          if (performance.now() - started > 3000) return resolve('still open after 3s');
          setTimeout(poll, 5);
        };
        poll();
      }));
      assert.deepEqual(leaving, { layer: 'ember-layer-out', scrim: 'ember-fade-out' });
      await page.waitForFunction(() => !document.querySelector('[role=dialog]'));
      // With the preference set nothing is deferred. The kill switch has to stay at least as
      // specific as the per-state rules; written as a bare class it loses to them silently.
      await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
      await open();
      assert.equal(await page.$eval('.ember-dialog', e => getComputedStyle(e).animationName), 'none');
    });
    await t.test("context inspector stays concise and rejects obsolete snapshots", async () => {
      await page.setViewport({ width: 640, height: 540, deviceScaleFactor: 1 });
      await page.goto(`${origin}/__ember-test/context`);
      // Navigation can finish before the dynamically imported fixture initializes.
      await page.waitForFunction(() => window.__contextFixture?.pending.length === 1);
      const snapshot = { runId: 11, project: "Ember", selection: "auto", reason: "Project path", profile: "Tone: direct", profileSources: [], profileReviewNeeded: false, profileInvalid: false, projectContext: "Stack: Rust", sources: [], sourceStatus: "Up to date", delivery: "prepared" };
      await page.evaluate(value => window.__contextFixture.pending.shift()(value), snapshot);
      await page.waitForFunction(() => document.body.textContent.includes('Ember · Automatic'));
      assert.equal(await page.$$eval('details', e => e.length), 0);
      assert.equal(await page.$eval('section', e => e.innerText.includes('Prepared')), true);
      await capture('context-collapsed');
      await page.click('button[aria-label="Context details"]');
      await page.waitForSelector('[role=dialog]');
      const details = await page.$eval('[role=dialog]', e => e.textContent);
      for (const expected of ['Request', '11', 'Up to date', 'Tone: direct', 'Stack: Rust']) assert.ok(details.includes(expected), expected);
      await capture('dialog-context-details');
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => !document.querySelector('[role=dialog]'));
      // Radix hands focus back to the trigger in a setTimeout(0) after the unmount, so reading
      // activeElement on the very next frame raced it: green here, red on every CI runner.
      // Waiting asserts the same thing (focus returns) without betting on the runner's clock.
      await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'Context details');
      await page.click('button[aria-label="Refresh context"]');
      await page.waitForFunction(() => window.__contextFixture.pending.length === 1);
      await page.evaluate(value => window.__contextFixture.pending.shift()(value), { ...snapshot, runId: 10, project: 'Obsolete', delivery: 'sent' });
      await presented();
      assert.equal(await page.$eval('section', e => e.innerText.includes('Obsolete')), false);
      await page.click('button[aria-label="Refresh context"]');
      await page.waitForFunction(() => window.__contextFixture.pending.length === 1);
      await page.evaluate(value => window.__contextFixture.pending.shift()(value), { ...snapshot, delivery: 'sent' });
      await page.waitForFunction(() => document.querySelector('section').innerText.includes('Sent'));
      await capture('context-expanded');
    });
    await t.test("project distillation and colour responses preserve the current draft", () => projectRegressions(page, `${origin}`, capture));
    await t.test("settings preserve busy labels, contrast and keyboard navigation in both themes", () => settingsRegressions(page, `${origin}`, capture));
    await t.test("each tab shows the data it already had", () => tabContentRegressions(page, `${origin}`));
    await t.test("the overlay preview keeps its own palette and its orb is visible", () => appearanceRegressions(page, `${origin}`, capture));
    await t.test("every tab fits the window at three sizes in both themes", () => settingsLayoutRegressions(page, `${origin}`, capture));
    // About grows by a line the moment an update check answers, and that line was enough to push
    // the GitHub link out through the bottom of its own card. The matrix never saw it because
    // the check only runs when you press the button.
    await t.test("About still fits once an update check has answered", async () => {
      await settingsLayoutRegressions(page, `${origin}`, async name => capture(`${name}-checked`), ['about'], async () => {
        await openTab(page, 'About');
        await page.waitForSelector('#debug-mode');
        await page.evaluate(() => {
          const button = Array.from(document.querySelectorAll('button')).find(b => b.textContent.trim() === 'Check for updates');
          if (!button) throw new Error('no Check for updates button');
          button.click();
        });
        await page.waitForSelector('[role=alert], [role=status]');
      });
    });

    // The Developer tab is the one panel the matrix above cannot reach: it does not exist until
    // the switch in About is on, and it carries the tallest content in the app.
    await t.test("the Developer tab fits once its switch is on", async () => {
      await settingsLayoutRegressions(page, `${origin}`, async name => capture(`${name}-devtools`), ['about', 'dev'], async () => {
        await openTab(page, 'About');
        await page.waitForSelector('#debug-mode');
        await page.click('#debug-mode');
        await page.waitForSelector('[role=tab][data-state=active]');
      });
    });
}));
