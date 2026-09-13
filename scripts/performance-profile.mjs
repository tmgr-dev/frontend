import { chromium } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import { mockApp } from '../tests/performance/mockApp.mjs';

const baseURL = process.env.PERF_URL || 'http://127.0.0.1:4179';
const label = process.env.PERF_LABEL || 'current';
const directory = process.env.PERF_OUTPUT || 'performance-results';
await mkdir(directory, { recursive: true });
const browser = await chromium.launch({ channel: 'chrome' });
const results = [];
for (const taskCount of [20, 200, 1000]) {
  const context = await browser.newContext({
    baseURL,
    serviceWorkers: 'block',
    viewport: { width: 1440, height: 900 },
  });
  const page = await context.newPage();
  await mockApp(page, { taskCount });
  await page.addInitScript(() => {
    window.__profile = { longTasks: [], shifts: [] };
    new PerformanceObserver((list) =>
      window.__profile.longTasks.push(
        ...list.getEntries().map((e) => e.duration),
      ),
    ).observe({ type: 'longtask', buffered: true });
    new PerformanceObserver((list) =>
      window.__profile.shifts.push(
        ...list
          .getEntries()
          .filter((e) => !e.hadRecentInput)
          .map((e) => e.value),
      ),
    ).observe({ type: 'layout-shift', buffered: true });
  });
  const cdp = await context.newCDPSession(page);
  await cdp.send('Performance.enable');
  await cdp.send('Tracing.start', {
    categories: 'devtools.timeline,v8,blink.user_timing',
    transferMode: 'ReturnAsStream',
  });
  const route = async (path) => {
    await page.evaluate(
      (path) =>
        document
          .querySelector('#app')
          .__vue_app__.config.globalProperties.$router.push(path),
      path,
    );
  };
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  const initialResources = await page.evaluate(() =>
    performance
      .getEntriesByType('resource')
      .filter((e) => /\.(js|css|woff2)(\?|$)/.test(e.name))
      .map((e) => ({
        name: new URL(e.name).pathname,
        transfer: e.transferSize,
        decoded: e.decodedBodySize,
      })),
  );
  const cycleSnapshots = [];
  const started = Date.now();
  await route('/demo/board');
  await page.locator('.board-card-draggable').first().waitFor();
  await page
    .locator('.board-card-draggable')
    .getByText('Original task', { exact: true })
    .first()
    .waitFor();
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  const boardReadyMs = Date.now() - started;
  await cdp.send('HeapProfiler.collectGarbage');
  const before = await cdp.send('Performance.getMetrics');
  // Repeat complete page lifecycle, with actual rendered data, 30 times on the medium fixture.
  if (taskCount === 200)
    for (let i = 0; i < 30; i++) {
      await route('/demo/list');
      await page.locator('[data-task-id="1"]').waitFor();
      await route('/demo/board');
      await page
        .locator('.board-card-draggable')
        .getByText('Original task', { exact: true })
        .first()
        .waitFor();
      if ((i + 1) % 10 === 0) {
        await page.evaluate(
          () =>
            new Promise((resolve) =>
              requestAnimationFrame(() => requestAnimationFrame(resolve)),
            ),
        );
        await cdp.send('HeapProfiler.collectGarbage');
        cycleSnapshots.push({
          cycle: i + 1,
          metrics: (await cdp.send('Performance.getMetrics')).metrics.filter(
            (m) =>
              ['Nodes', 'JSHeapUsedSize', 'JSEventListeners'].includes(m.name),
          ),
        });
      }
    }
  await page.evaluate(() => {
    const el = document.querySelector('.board-container');
    if (el) el.scrollTop = el.scrollHeight;
  });
  await cdp.send('HeapProfiler.collectGarbage');
  const after = await cdp.send('Performance.getMetrics');
  const profile = await page.evaluate(() => ({
    ...window.__profile,
    resources: performance
      .getEntriesByType('resource')
      .filter((e) => /\.(js|css|woff2)(\?|$)/.test(e.name))
      .map((e) => ({
        name: new URL(e.name).pathname,
        transfer: e.transferSize,
        decoded: e.decodedBodySize,
      })),
  }));
  const completed = new Promise((resolve) =>
    cdp.once('Tracing.tracingComplete', resolve),
  );
  await cdp.send('Tracing.end');
  const { stream } = await completed;
  let trace = '';
  for (;;) {
    const chunk = await cdp.send('IO.read', { handle: stream });
    trace += chunk.data;
    if (chunk.eof) break;
  }
  await cdp.send('IO.close', { handle: stream });
  await writeFile(`${directory}/${label}-${taskCount}.trace.json`, trace);
  const metrics = (value) =>
    Object.fromEntries(
      value.metrics
        .filter((m) =>
          ['JSHeapUsedSize', 'Nodes', 'Documents', 'JSEventListeners'].includes(
            m.name,
          ),
        )
        .map((m) => [m.name, m.value]),
    );
  results.push({
    taskCount,
    initialResources,
    cycleSnapshots,
    cycles: taskCount === 200 ? 30 : 0,
    boardReadyMs,
    before: metrics(before),
    after: metrics(after),
    ...profile,
  });
  await context.close();
}
await browser.close();
await writeFile(
  `${directory}/${label}.json`,
  JSON.stringify(
    {
      browser: 'installed Chrome on macOS',
      viewport: '1440x900',
      mode: 'production preview',
      results,
    },
    null,
    2,
  ),
);
console.log(
  JSON.stringify(
    results.map(({ taskCount, boardReadyMs, before, after, longTasks }) => ({
      taskCount,
      boardReadyMs,
      before,
      after,
      longTasks: longTasks.length,
    })),
    null,
    2,
  ),
);
