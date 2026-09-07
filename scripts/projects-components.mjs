import assert from "node:assert/strict";

export async function projectRegressions(page, origin, capture = async () => {}) {
  await page.setViewport({ width: 1000, height: 640, deviceScaleFactor: 1 });
  await page.goto(`${origin}/__ember-test/projects`);
  await page.waitForSelector('button[aria-label="Edit"]');
  const click = async label => page.evaluate(label => {
    const button = Array.from(document.querySelectorAll('button')).find(b => b.textContent.trim() === label);
    if (!button || button.disabled) throw new Error(`Button unavailable: ${label}`);
    button.click();
  }, label);
  const edit = async id => {
    await page.evaluate(id => {
      const names = { a: 'Alpha', b: 'Beta' };
      const card = Array.from(document.querySelectorAll('button[aria-label="Edit"]')).find(button =>
        button.parentElement.textContent.includes(names[id]));
      if (!card || card.disabled) throw new Error('Editor unavailable');
      card.click();
    }, id);
    // The editor pane is created on select, so wait for the section itself before anything
    // measures geometry: the grid goes from one column to two as it appears.
    await page.waitForSelector('[aria-label="Project editor"]');
    await page.waitForSelector(`#name-${id}`);
  };

  // No editor pane exists until a project is selected, and the list spans the whole tab.
  assert.equal(await page.$('[aria-label="Project editor"]'), null);
  const short = await page.evaluate(() => {
    const list = document.querySelector('[aria-label="Projects"]').getBoundingClientRect();
    const body = document.querySelector('[data-tab-body]').getBoundingClientRect();
    return Math.round(body.width - list.width);
  });
  assert.ok(short <= 1, `the list should span the tab, short by ${short}px`);

  await edit('a');
  assert.ok(await page.$('[aria-label="Project editor"]'));
  await page.click('button[aria-label="Manage automatic context"]');
  await page.waitForFunction(() => document.querySelector('[role=dialog]')?.innerText.includes('AGENTS.md'));
  assert.equal(await page.evaluate(() => document.body.innerText.includes('/fixture/Alpha/AGENTS.md')), false);
  await page.evaluate(() => Array.from(document.querySelectorAll('[role=dialog] button')).find(e => e.textContent === 'AGENTS.md').focus());
  await page.keyboard.press('Enter');
  assert.equal(await page.evaluate(() => document.body.innerText.includes('/fixture/Alpha/AGENTS.md')), true);
  await capture('dialog-project-context');
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('[role=dialog]'));
  for (const theme of ['dark', 'cream']) {
    await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
    await capture(`projects-${theme}`);
  }
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });

  await click('Check project sources');
  await page.waitForFunction(() => document.body.textContent.includes('Generate a reviewed draft'));
  await click('Generate a reviewed draft');
  await page.waitForFunction(() => window.__projectsFixture.distillations === 1);
  await edit('b');
  await page.evaluate(() => window.__projectsFixture.resolveDistillation('Obsolete Alpha result'));
  await page.waitForFunction(() => !Array.from(document.querySelectorAll('button')).find(b => b.textContent.trim() === 'Check project sources')?.disabled);
  assert.equal(await page.$eval('#brief-b', e => e.value), 'Brief for Beta');

  await edit('a');
  await page.$eval('button[aria-label="Custom colour"]', e => e.scrollIntoView({ block: 'center', behavior: 'instant' }));
  // The editor expands with animation. A native pointer click must target its settled
  // position, otherwise pointer-down and pointer-up can land on different controls.
  await page.waitForFunction(() => {
    const button = document.querySelector('button[aria-label="Custom colour"]');
    const r = button.getBoundingClientRect();
    const position = `${r.x},${r.y},${r.width},${r.height}`;
    const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    const f = window.__projectsFixture;
    f.buttonFrames = f.buttonPosition === position && hit && button.contains(hit) ? (f.buttonFrames ?? 0) + 1 : 0;
    f.buttonPosition = position;
    return f.buttonFrames >= 3;
  });
  await page.click('button[aria-label="Custom colour"]');
  await page.waitForSelector('[aria-label="Colour wheel"]');
  await page.$eval('[aria-label="Colour wheel"]', element => element.scrollIntoView({ block: 'center', behavior: 'instant' }));
  await page.waitForFunction(() => {
    const wheel = document.querySelector('[aria-label="Colour wheel"]');
    const r = wheel.getBoundingClientRect();
    const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    const position = `${r.x},${r.y},${r.width},${r.height}`;
    const f = window.__projectsFixture;
    f.stableFrames = f.position === position && hit && wheel.contains(hit) ? (f.stableFrames ?? 0) + 1 : 0;
    f.position = position;
    return f.stableFrames >= 3;
  });
  const box = await page.$eval('[aria-label="Colour wheel"]', element => {
    const r = element.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  });
  await page.mouse.move(box.x, box.y);
  await page.mouse.down();
  await page.mouse.up();
  await page.waitForFunction(() => window.__projectsFixture.wheel.length >= 2);
  await click('Done');
  // Closing the colour popover hands focus back to its trigger in a setTimeout(0) (Radix
  // FocusScope). Focusing the name field before that fires kept the first letter and sent the
  // rest to the swatch, which is what "A" instead of "Alpha revised" on a slow runner was.
  await page.waitForFunction(() => !document.querySelector('.ember-popover'));
  await page.evaluate(() => new Promise(resolve => setTimeout(resolve, 0)));
  await page.focus('#name-a');
  await page.waitForFunction(() => document.activeElement?.id === 'name-a');
  await page.$eval('#name-a', e => e.select());
  await page.keyboard.type('Alpha revised');
  await page.evaluate(() => {
    const list = window.__projectsFixture.wheel;
    list[list.length - 1]({ raw: '#111111', mid: '#123456', glow: '#ffffff', chroma: 0.2, hue: 40 });
  });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.equal(await page.$eval('#name-a', e => e.value), 'Alpha revised');
  await click('Save');
  await page.waitForFunction(() => window.__projectsFixture.saved.length === 1);
  const saved = await page.evaluate(() => window.__projectsFixture.saved[0]);
  assert.equal(saved.name, 'Alpha revised');
  assert.equal(saved.accentCustom, '#123456');
  assert.equal(saved.brief, 'Brief for Alpha');
  // Closing removes the pane rather than hiding it, so the list goes back to the full width.
  await page.waitForFunction(() => !document.querySelector('[aria-label="Project editor"]'));
}
