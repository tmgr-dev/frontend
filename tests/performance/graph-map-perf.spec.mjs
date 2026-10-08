import { expect, test } from '@playwright/test';
import { mkdirSync, writeFileSync } from 'node:fs';
import { buildGraphMap } from './graphMapFixture.mjs';
import { debugInfo, setupMap } from './graphMapHelpers.mjs';

const notesDir = process.env.GRAPH_SHOTS;

const percentile = (list, p) => {
  const sorted = [...list].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))];
};

test('3000 nodes and 6000 links: layout settle time and pan/zoom frame rate', async ({
  page,
}) => {
  test.setTimeout(120000);
  const map = buildGraphMap({
    tasks: 2500,
    pages: 500,
    links: 6000,
    orphans: 30,
    seed: 11,
  });
  await page.setViewportSize({ width: 1440, height: 900 });
  await setupMap(page, map);
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/demo/map');
  const canvas = page.getByTestId('map-canvas');
  await page.getByTestId('range-90').click();
  await page.waitForSelector('[data-testid="map-canvas"][data-settled="1"]', {
    timeout: 90000,
  });
  const layoutMs = Number(await canvas.getAttribute('data-layout-ms'));
  await page.waitForTimeout(1500);
  const info = await debugInfo(canvas);
  const shown = info.visible + info.rings;

  if (notesDir)
    await page.screenshot({ path: `${notesDir}/map-3000-dark.png` });
  const box = await canvas.boundingBox();
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.evaluate(() => {
    window.__frames = [];
    let last = performance.now();
    const tick = (t) => {
      window.__frames.push(t - last);
      last = t;
      window.__raf = requestAnimationFrame(tick);
    };
    window.__raf = requestAnimationFrame(tick);
  });
  await page.mouse.move(cx, cy);
  for (let i = 0; i < 40; i++) {
    await page.mouse.wheel(0, i < 20 ? -120 : 120);
    await page.waitForTimeout(16);
  }
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  for (let i = 0; i < 90; i++) {
    await page.mouse.move(
      cx + Math.sin(i / 8) * 220,
      cy + Math.cos(i / 8) * 120,
    );
    await page.waitForTimeout(16);
  }
  await page.mouse.up();
  const frames = await page.evaluate(() => {
    cancelAnimationFrame(window.__raf);
    return window.__frames.slice(2);
  });
  const avg = frames.reduce((a, b) => a + b, 0) / frames.length;
  const result = {
    nodes: map.nodes.length,
    edges: map.edges.length,
    shown,
    layoutMs,
    frames: frames.length,
    avgFrameMs: avg,
    fps: 1000 / avg,
    p95Ms: percentile(frames, 0.95),
    maxMs: Math.max(...frames),
    over20: frames.filter((f) => f > 20).length,
  };
  console.log(JSON.stringify(result));
  if (notesDir) {
    mkdirSync(notesDir, { recursive: true });
    writeFileSync(
      `${notesDir}/frontend-perf.md`,
      `# Workspace map: frontend performance\n\n` +
        `Synthetic map: ${result.nodes} nodes, ${result.edges} edges, 6 clusters, 30 unlinked items (Last 90 days).\n` +
        `Chrome (headless, 1440x900, dev server), Web Worker layout.\n\n` +
        `| Metric | Value |\n|---|---|\n` +
        `| Layout settle (worker, wall clock) | ${(layoutMs / 1000).toFixed(
          2,
        )} s |\n` +
        `| Items drawn after the range filter | ${shown} |\n` +
        `| Frames sampled during wheel zoom + drag pan | ${result.frames} |\n` +
        `| Average frame | ${result.avgFrameMs.toFixed(
          1,
        )} ms (${result.fps.toFixed(0)} fps) |\n` +
        `| p95 frame | ${result.p95Ms.toFixed(1)} ms |\n` +
        `| Worst frame | ${result.maxMs.toFixed(1)} ms |\n` +
        `| Frames over 20 ms | ${result.over20} |\n`,
    );
  }
  expect(layoutMs).toBeLessThan(15000);
  expect(result.fps).toBeGreaterThan(30);
});
