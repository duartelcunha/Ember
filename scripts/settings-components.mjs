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

const TAB_LABELS = { providers: 'Providers', refining: 'Refining', hotkey: 'Shortcut', projects: 'Projects', profile: 'Profile', appearance: 'Appearance', about: 'About' };
export const LAYOUT_SIZES = [[720, 520], [720, 856], [1000, 640], [1400, 900]];

/**
 * Tabs that must OCCUPY their panel, not merely fit inside it. Grows as each tab is converted;
 * the final step deletes the set so the rule is unconditional.
 */
const FILLS = new Set(['hotkey', 'projects']);

/**
 * The settings never scroll as a page. For every tab, at the minimum window, the default and a
 * large one, in both themes: the document and `main` do not scroll, no native <details> is
 * left, the size container has a height, and every element of the tab body lies inside `main`
 * unless it sits in a pane that is allowed to scroll (`data-scroll-pane`). The per-element
 * check is the real one: `main` is `overflow-hidden`, so its scrollHeight alone would pass.
 *
 * Fitting is not the same as looking designed, so tabs in FILLS are also held to the height
 * contract: the body is as tall as its panel, and no container carries meaningfully more air
 * below its children than above. A tab that fills passes with both gaps near zero, one that
 * centres passes with equal gaps, and the original defect (everything pinned to the top with
 * 40% empty beneath) fails.
 */
export async function settingsLayoutRegressions(page, origin, capture, tabs = Object.keys(TAB_LABELS)) {
  await page.goto(`${origin}/__ember-test/settings`);
  await page.waitForSelector('[role=tab]');
  const settle = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  for (const [width, height] of LAYOUT_SIZES) {
    await page.setViewport({ width, height, deviceScaleFactor: 1 });
    for (const theme of ['dark', 'cream']) {
      await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
      for (const tab of tabs) {
        // A real pointer click: Radix tabs activate on pointer-down, and a synthetic `click()`
        // left every capture on the first tab.
        const trigger = (await page.$$('[role=tab]'))[Object.keys(TAB_LABELS).indexOf(tab)];
        await trigger.click();
        await page.waitForFunction(label => document.querySelector('[role=tab][data-state=active]')?.textContent.includes(label), {}, TAB_LABELS[tab]);
        await page.waitForSelector('[data-tab-body]');
        await settle();
        const issues = await page.evaluate(() => {
          const issues = [];
          const main = document.querySelector('main');
          const mr = main.getBoundingClientRect();
          const doc = document.documentElement;
          if (doc.scrollHeight > doc.clientHeight || doc.scrollWidth > doc.clientWidth) issues.push(`document scrolls ${doc.scrollWidth}x${doc.scrollHeight}`);
          if (main.scrollHeight > main.clientHeight + 1) issues.push(`main scrolls ${main.scrollHeight} > ${main.clientHeight}`);
          if (document.querySelectorAll('details').length) issues.push('native <details> present');
          const viewport = document.querySelector('.settings-viewport');
          if (!viewport || viewport.clientHeight <= 0) issues.push('settings viewport has no height');
          const body = document.querySelector('[data-tab-body]');
          const describe = el => `${el.tagName.toLowerCase()}${el.id ? '#' + el.id : ''}${el.className && typeof el.className === 'string' ? '.' + el.className.split(/\s+/).slice(0, 3).join('.') : ''}`;
          for (const el of body.querySelectorAll('*')) {
            const pane = el.closest('[data-scroll-pane]');
            if (pane && pane !== el) continue;
            const r = el.getBoundingClientRect();
            if (r.width === 0 && r.height === 0) continue;
            if (r.bottom > mr.bottom + 1 || r.right > mr.right + 1 || r.top < mr.top - 1 || r.left < mr.left - 1) {
              issues.push(`${describe(el)} at ${Math.round(r.left)},${Math.round(r.top)} ${Math.round(r.width)}x${Math.round(r.height)} leaves main (${Math.round(mr.width)}x${Math.round(mr.height)})`);
            }
          }
          return issues;
        });
        assert.deepEqual(issues, [], `${tab} at ${width}x${height} (${theme})`);
        if (FILLS.has(tab)) {
          const air = await page.evaluate(() => {
            const out = [];
            const body = document.querySelector('[data-tab-body]');
            const panel = body.parentElement;
            const pr = panel.getBoundingClientRect(), br = body.getBoundingClientRect();
            if (br.height < pr.height - 1) out.push(`tab body ${Math.round(br.height)} shorter than panel ${Math.round(pr.height)}`);
            const measure = (container, name) => {
              const kids = [...container.children].filter(el => { const r = el.getBoundingClientRect(); return r.width || r.height; });
              if (!kids.length) return;
              const cr = container.getBoundingClientRect();
              const top = Math.min(...kids.map(el => el.getBoundingClientRect().top)) - cr.top;
              const bottom = cr.bottom - Math.max(...kids.map(el => el.getBoundingClientRect().bottom));
              if (bottom > Math.max(8, top + 8)) out.push(`${name}: ${Math.round(bottom)}px of air below, ${Math.round(top)}px above`);
            };
            measure(body, 'tab body');
            document.querySelectorAll('[data-settings-col]').forEach((col, i) => measure(col, `column ${i}`));
            return out;
          });
          assert.deepEqual(air, [], `${tab} at ${width}x${height} (${theme}) does not occupy its panel`);
        }
        await capture(`${tab}-${width}x${height}-${theme}`);
      }
    }
  }
}
