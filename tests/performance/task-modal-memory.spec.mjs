import { expect, test } from '@playwright/test';
import { instrument, recordSockets } from './memoryProbe.mjs';
import { mockApp } from './mockApp.mjs';

const CYCLES = 30;

const metrics = async (page) => {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Performance.enable');
  await cdp.send('HeapProfiler.collectGarbage');
  await cdp.send('HeapProfiler.collectGarbage');
  const { metrics: raw } = await cdp.send('Performance.getMetrics');
  await cdp.detach();
  const m = Object.fromEntries(raw.map((x) => [x.name, x.value]));
  const live = await page.evaluate(() => window.__live());
  return { nodes: m.Nodes, heap: m.JSHeapUsedSize, live };
};

test('opening and closing the task modal does not accumulate DOM, listeners or subscriptions', async ({
  page,
}) => {
  test.setTimeout(240000);
  const sockets = [];
  await mockApp(page);
  await recordSockets(page, sockets);
  await instrument(page);
  await page.goto('/demo/list');
  const row = page.locator('[data-task-id="1"]').first();
  await row.waitFor({ timeout: 60000 });
  const panel = page.locator('[data-name="task-side-panel"]');

  const cycle = async () => {
    await row.click();
    await page.getByPlaceholder('Task name').waitFor();
    await page.waitForTimeout(300);
    for (let i = 0; i < 8 && (await panel.count()); i++) {
      await page.keyboard.press('Escape');
      await page.waitForTimeout(400);
    }
    await expect(panel).toHaveCount(0);
  };

  for (let i = 0; i < 3; i++) await cycle();
  const before = await metrics(page);
  const subscribed = sockets.length;
  for (let i = 0; i < CYCLES; i++) await cycle();
  const after = await metrics(page);

  expect(after.nodes - before.nodes).toBeLessThan(CYCLES * 3);
  expect(after.heap - before.heap).toBeLessThan(15 * 1024 * 1024);
  for (const key of [
    'listener:window:resize',
    'listener:window:scroll',
    'listener:document:selectionchange',
    'listener:document:fullscreenchange',
    'interval',
    'worker',
    'eventsource',
  ])
    expect(after.live[key] ?? 0, key).toBe(before.live[key] ?? 0);
  expect(sockets.length).toBe(subscribed);
});
