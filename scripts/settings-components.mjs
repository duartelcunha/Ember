import assert from 'node:assert/strict';

export async function settingsRegressions(page, origin, capture) {
  for (const theme of ['dark', 'cream']) {
    await page.setViewport({ width: 1000, height: 800, deviceScaleFactor: 1 });
    await page.goto(`${origin}/__ember-test/settings`);
    await page.waitForSelector('#gemini-key');
    await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
    await page.type('#gemini-key', 'fixture');
    const save = await page.$('#gemini-key + button');
    const before = await save.boundingBox();
    await save.click();
    await page.waitForFunction(() => window.__settingsFixture.keyPending);
    const during = await save.boundingBox();
    assert.equal(during.width, before.width);
    assert.equal(await save.evaluate(e => e.getAttribute('aria-busy')), 'true');
    assert.equal(await save.evaluate(e => e.innerText.trim()), 'Save');
    await page.evaluate(() => window.__settingsFixture.resolveKey());
    await page.waitForSelector('[role=alert]');
    const contrast = await page.evaluate(() => {
      const lum = rgb => {
        const channels = rgb.match(/[\d.]+/g).slice(0, 3).map(Number).map(n => n / 255).map(n => n <= .04045 ? n / 12.92 : ((n + .055) / 1.055) ** 2.4);
        return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
      };
      return ['[role=alert]', '#gemini-key + button'].map(selector => {
        const style = getComputedStyle(document.querySelector(selector));
        const a = lum(style.color), b = lum(style.backgroundColor);
        return (Math.max(a, b) + .05) / (Math.min(a, b) + .05);
      });
    });
    for (const ratio of contrast) assert.ok(ratio >= 4.5, `${theme} contrast ${ratio}`);
    await capture(`settings-${theme}`);
    await page.setViewport({ width: 720, height: 640, deviceScaleFactor: 1.5 });
    const tabsFit = await page.$$eval('[role=tab]', tabs => tabs.every(tab => {
      const r = tab.getBoundingClientRect();
      return r.left >= 0 && r.right <= innerWidth && tab.scrollWidth <= tab.clientWidth + 1;
    }));
    assert.equal(tabsFit, true);
    await page.focus('[role=tab][data-state=active]');
    await page.keyboard.press('ArrowRight');
    await page.waitForFunction(() => document.querySelector('[role=tab][data-state=active]')?.textContent.includes('Refining'));
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('role')), 'tab');
    await capture(`refining-${theme}`);
    await page.keyboard.press('End');
    await page.waitForFunction(() => document.body.textContent.includes('Check for updates'));
    await page.evaluate(() => Array.from(document.querySelectorAll('button')).find(e => e.textContent.includes('Check for updates')).click());
    await page.waitForSelector('[role=alert]');
    assert.ok(await page.$eval('[role=alert]', e => e.textContent.includes("Couldn't check for updates")));
  }
}
