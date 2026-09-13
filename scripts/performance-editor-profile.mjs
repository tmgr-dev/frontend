import { chromium } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import { mockApp } from '../tests/performance/mockApp.mjs';

const browser = await chromium.launch({ channel: 'chrome' });
const results = [];
for (const [label, baseURL] of [
  ['baseline', 'http://127.0.0.1:4180'],
  ['optimized', 'http://127.0.0.1:4179'],
]) {
  const context = await browser.newContext({
    baseURL,
    serviceWorkers: 'block',
  });
  const page = await context.newPage();
  await mockApp(page);
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  const started = Date.now();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/tasks/1'),
  );
  await page.getByPlaceholder('Task name').waitFor();
  const formReadyMs = Date.now() - started;
  await page.locator('.md-editor').first().waitFor();
  results.push({ label, formReadyMs, editorReadyMs: Date.now() - started });
  await context.close();
}
await browser.close();
await mkdir('performance-results', { recursive: true });
await writeFile(
  'performance-results/editor.json',
  JSON.stringify(results, null, 2),
);
console.log(JSON.stringify(results, null, 2));
