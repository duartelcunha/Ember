import { test } from "node:test";
import assert from "node:assert/strict";
import { withBrowser } from "./browser-harness.mjs";
import { projectRegressions } from "./projects-components.mjs";
import { appearanceRegressions, settingsLayoutRegressions, settingsRegressions, tabContentRegressions } from "./settings-components.mjs";

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
      assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('aria-label')), 'Context details');
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
}));
