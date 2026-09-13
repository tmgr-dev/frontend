import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('file previews wait for visibility, reserve geometry and keep four requests maximum', async ({
  page,
}) => {
  await mockApp(page);
  const files = Array.from({ length: 60 }, (_, i) => ({
    id: i + 1,
    name: `Image ${i + 1}`,
    mime_type: 'image/png',
    size: 100,
    task: { id: 1, title: 'Original task', key: 'T1' },
  }));
  await page.route('**/api/workspaces/1/files*', (route) =>
    route.fulfill({
      json: { data: files, meta: { total: 60, last_page: 1, current_page: 1 } },
    }),
  );
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  const requested = [];
  let active = 0,
    maximum = 0;
  await page.route('**/api/files/*/signed-url', async (route) => {
    const id = Number(new URL(route.request().url()).pathname.split('/')[3]);
    requested.push(id);
    maximum = Math.max(maximum, ++active);
    await gate;
    active--;
    await route.fulfill({
      json: {
        data: {
          url: `/api/files/${id}/content?signed=fixture`,
          expires_at: '2099-01-01T00:00:00Z',
        },
      },
    });
  });
  await page.route('**/api/files/*/content*', (route) =>
    route.fulfill({
      contentType: 'image/png',
      body: Buffer.from(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aKzsAAAAASUVORK5CYII=',
        'base64',
      ),
    }),
  );
  await page.goto('/');
  await page.locator('[data-task-id="1"]').waitFor();
  await page.evaluate(() =>
    document
      .querySelector('#app')
      .__vue_app__.config.globalProperties.$router.push('/demo/files'),
  );
  const first = page.locator('.workspace-files li').first();
  await expect(first).toBeVisible();
  await expect.poll(() => requested.length).toBe(4);
  const initialHeight = (await first.boundingBox()).height;
  expect(requested).not.toContain(60);
  release();
  await expect(first.locator('img')).toBeVisible();
  expect((await first.boundingBox()).height).toBe(initialHeight);
  await page.getByText('Image 60', { exact: true }).scrollIntoViewIfNeeded();
  await expect.poll(() => requested.includes(60)).toBe(true);
  expect(maximum).toBeLessThanOrEqual(4);
});
