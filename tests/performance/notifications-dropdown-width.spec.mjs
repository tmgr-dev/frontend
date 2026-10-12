import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const long = 'word '.repeat(80);
const unbroken = 'https://example.test/' + 'a'.repeat(300);

const notification = (id, title, message) => ({
  id,
  user_id: 1,
  workspace_id: 1,
  activity_id: null,
  type: 'task_updated',
  title,
  message,
  data: null,
  link: null,
  read_at: null,
  created_at: '2026-10-01T10:00:00Z',
  updated_at: '2026-10-01T10:00:00Z',
});

for (const [w, h] of [
  [1600, 900],
  [390, 844],
]) {
  test(`dropdown stays within bounds at ${w}x${h}`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await mockApp(page);
    await page.route('**/api/notifications**', (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path.endsWith('/unread-count'))
        return route.fulfill({ json: { count: 2 } });
      return route.fulfill({
        json: {
          data: [
            notification(1, long, long),
            notification(2, unbroken, unbroken),
          ],
          current_page: 1,
          last_page: 1,
          per_page: 20,
          total: 2,
        },
      });
    });

    await page.goto('/demo/board');
    await page.getByRole('button', { name: 'Notifications' }).first().click();
    const dropdown = page.locator('.notification-dropdown');
    await expect(dropdown).toBeVisible();
    await expect(dropdown.locator('.notification-item')).toHaveCount(2);

    const box = await dropdown.boundingBox();
    expect(box.width).toBeLessThanOrEqual(Math.min(380, w - 16) + 0.5);
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(w + 0.5);

    const overflow = await dropdown.evaluate((el) => {
      const bad = [el, ...el.querySelectorAll('*')].filter(
        (n) => n.scrollWidth > n.clientWidth + 1 && n.clientWidth > 0,
      );
      return bad.map((n) => n.className);
    });
    expect(overflow).toEqual([]);
  });
}
