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

/**
 * The overlay preview shows the real components, so two things can silently break it.
 *
 * The theme: `.ember-bubble` follows `data-theme`, and the overlay window never has one, so a
 * preview that went cream inside a cream settings window would depict something the user will
 * never see. Asserted by computing the same styles under both themes.
 *
 * The orb: it renders through motion's `m.div`, whose features only load under a `LazyMotion`
 * ancestor. Without one it mounts at opacity 0. The whole suite runs under reduced motion, where
 * the orb's own still branch starts it opaque and hides exactly that bug, so this flips the
 * emulation and puts it back.
 */
export async function appearanceRegressions(page, origin, capture) {
  await page.setViewport({ width: 1000, height: 640, deviceScaleFactor: 1 });
  await page.goto(`${origin}/__ember-test/settings`);
  await page.waitForSelector('[role=tab]');
  const openAppearance = async () => {
    const trigger = (await page.$$('[role=tab]'))[5];
    await trigger.click();
    await page.waitForSelector('[data-overlay-preview]');
  };
  const pick = value => page.evaluate(v => {
    document.querySelector(`input[name="preview-state"][value="${v}"]`).click();
  }, value);
  await openAppearance();

  await pick('confirm');
  await page.waitForSelector('[data-overlay-preview] .ember-bubble');
  const bubble = () => page.evaluate(() => {
    const s = getComputedStyle(document.querySelector('[data-overlay-preview] .ember-bubble'));
    return [s.backgroundColor, s.color, s.borderTopColor];
  });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  const dark = await bubble();
  assert.equal(dark[0], 'rgb(27, 23, 19)', 'the preview bubble must use the dark surface');
  await capture('appearance-preview-dark');
  await page.evaluate(() => { document.documentElement.dataset.theme = 'cream'; });
  assert.deepEqual(await bubble(), dark, 'the overlay preview must not follow the settings theme');
  await capture('appearance-preview-cream');
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });

  // The orb, under the media state that would expose a missing LazyMotion.
  await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
  await page.reload();
  await page.waitForSelector('[role=tab]');
  await openAppearance();
  await pick('refining');
  await page.waitForSelector('[data-overlay-preview] svg');
  await page.evaluate(() => new Promise(resolve => setTimeout(resolve, 500)));
  const orb = await page.$eval('[data-overlay-preview] svg', e => {
    const box = e.getBoundingClientRect();
    return { opacity: getComputedStyle(e.parentElement).opacity, width: box.width, height: box.height };
  });
  assert.equal(orb.opacity, '1', 'the orb needs a LazyMotion ancestor or it never fades in');
  assert.ok(orb.width > 0 && orb.height > 0, `the orb has no box: ${JSON.stringify(orb)}`);
  await page.emulateMediaFeatures([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
}

/**
 * Each tab shows the thing it claims to show, and the data it already had. These are content
 * assertions, not layout ones: they fail if a refactor quietly drops the comparison back to a
 * dropdown, hides the shortcut descriptions, stops reading ProviderHealth, or puts the
 * diagnostics report back behind a button.
 */
export async function tabContentRegressions(page, origin) {
  await page.setViewport({ width: 1000, height: 640, deviceScaleFactor: 1 });
  await page.goto(`${origin}/__ember-test/settings`);
  await page.waitForSelector('[role=tab]');
  const open = async (index, ready) => {
    const trigger = (await page.$$('[role=tab]'))[index];
    await trigger.click();
    await page.waitForSelector(ready);
  };

  // Providers: the try order, fed by the health fields the tab used to discard.
  await open(0, '[data-try-order]');
  const strip = await page.$eval('[data-try-order]', e => e.textContent);
  assert.match(strip, /Gemini/, `the strip should name the primary first: ${strip}`);
  assert.match(strip, /No pre-validated fallback/, `the strip should carry the health verdict: ${strip}`);

  // Refining: three radios, all three outputs on screen, arrows move the selection.
  await open(1, 'input[name="refine-mode"]');
  assert.equal(await page.$$eval('input[name="refine-mode"]', e => e.length), 3);
  const outputs = await page.$$eval('.mode-example', e => e.map(x => x.textContent).join(' | '));
  for (const expected of ['Set up a meeting', 'Schedule a meeting', 'scheduling assistant']) {
    assert.ok(outputs.includes(expected), `every mode's example should be on screen: ${outputs}`);
  }
  await page.evaluate(() => window.__settingsFixture.modes.length = 0);
  await page.focus('input[name="refine-mode"]:checked');
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => window.__settingsFixture.modes.length >= 1);
  // The border transitions over 150ms; wait for it to settle instead of reading mid-fade.
  await page.waitForFunction(() => {
    const labels = [...document.querySelectorAll('.mode-option')];
    const picked = labels.find(l => l.querySelector('input').checked);
    const other = labels.find(l => l !== picked);
    return picked && other && getComputedStyle(picked).borderTopColor !== getComputedStyle(other).borderTopColor;
  }, { timeout: 3000 }).catch(() => { throw new Error('the chosen mode needs a visible difference, not only a checked input'); });

  // Shortcut: all four in one list, each saying what it does.
  await open(2, '[aria-label="Global shortcut shortcut"]');
  assert.equal(await page.$$eval('[aria-label$=" shortcut"]', e => e.length), 4);
  assert.ok(!(await page.$$eval('button', b => b.some(x => x.textContent.trim() === 'Set shortcut'))),
    'the box is the only capture affordance; a Set shortcut button duplicates it');
  assert.ok(await page.evaluate(() =>
    document.querySelector('[data-tab-body]').innerText.includes('Fixes spelling and wording')));

  // About: the product first; the report only after Developer tools is switched on.
  await open(6, 'img[alt="Ember"]');
  assert.equal(await page.$('[aria-label="Diagnostics report"]'), null, 'diagnostics must stay behind the gate');
  await page.click('#debug-mode');
  await page.waitForSelector('[aria-label="Diagnostics report"]');
  const report = await page.$eval('[aria-label="Diagnostics report"]', e => ({
    tag: e.tagName, pane: e.hasAttribute('data-scroll-pane'), text: e.textContent, editable: e.isContentEditable,
  }));
  assert.equal(report.tag, 'PRE');
  assert.ok(report.pane, 'the report must be a scroll pane or it overflows a short window');
  assert.ok(!report.editable);
  assert.ok(report.text.includes('Ember 1.1.0-test'), report.text.slice(0, 60));
  assert.ok(await page.$$eval('button', b => b.some(x => x.textContent.trim() === 'Copy')));
}

const TAB_LABELS = { providers: 'Providers', refining: 'Refining', hotkey: 'Shortcut', projects: 'Projects', profile: 'Profile', appearance: 'Appearance', about: 'About' };
export const LAYOUT_SIZES = [[720, 520], [720, 856], [1000, 640], [1400, 900]];

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
export async function settingsLayoutRegressions(page, origin, capture, tabs = Object.keys(TAB_LABELS), prepare = null) {
  await page.goto(`${origin}/__ember-test/settings`);
  await page.waitForSelector('[role=tab]');
  // A hook to put the page in a state the default matrix does not reach (a toggle switched on).
  if (prepare) await prepare();
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
        {
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
              // 24px of tolerance: a column a few pixels shorter than the sibling that sets the
              // row height is stretch slack, not the defect. The defect measured 82 to 400px.
              if (bottom > Math.max(24, top + 24)) out.push(`${name}: ${Math.round(bottom)}px of air below, ${Math.round(top)}px above`);
            };
            measure(body, 'tab body');
            // Columns are only held to the rule when the tab has NO elastic card. With one, that
            // card anchors the layout and a sidebar column beside it is allowed its slack, the
            // way a settings sidebar sits next to a tall content panel. Without one, every
            // column must be centred, or the tab is back to hugging the top.
            if (!body.querySelector('[data-elastic]')) {
              document.querySelectorAll('[data-settings-col]').forEach((col, i) => measure(col, `column ${i}`));
            }
            return out;
          });
          assert.deepEqual(air, [], `${tab} at ${width}x${height} (${theme}) does not occupy its panel`);
        }
        await capture(`${tab}-${width}x${height}-${theme}`);
      }
    }
  }
}
