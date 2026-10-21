import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// TM-686: the comment composer (modal footer + page right rail) is a textarea that grows up to
// 128px and then scrolls; AI step chips stay in one horizontally scrolling row.

const surfaces = {
  modal: async (page) => {
    await page.goto('/demo/list');
    await page.locator('[data-task-id="1"]').first().click();
    return page.locator('.new-form-container footer textarea');
  },
  page: async (page) => {
    await page.goto('/demo/tasks/1');
    return page.locator('aside textarea[placeholder="Write a comment…"]');
  },
};

const setup = async (page, { aiReply = '0' } = {}) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  const user = {
    id: 1,
    name: 'Test User',
    email: 'test@example.test',
    settings: [
      { id: 5, key: 'current_workspace', value: '1' },
      { id: 20, key: 'comment_ai_reply', value: aiReply },
    ],
  };
  const posted = [];
  const asked = [];
  await page.route('**/api/user', (route) =>
    route.fulfill({ json: { data: user } }),
  );
  await page.route('**/api/tasks/1/comments**', async (route) => {
    const req = route.request();
    if (req.method() === 'POST') {
      const body = req.postDataJSON();
      (req.url().includes('/help') ? asked : posted).push(body);
    }
    await route.fulfill({ json: { data: [] } });
  });
  return { posted, asked };
};

const box = (loc) => loc.evaluate((el) => el.getBoundingClientRect().toJSON());

for (const [name, open] of Object.entries(surfaces)) {
  test.describe(`${name} composer`, () => {
    test.use({ viewport: { width: 1440, height: 900 } });

    test('Enter sends, Shift+Enter adds a line, the field grows, caps and resets', async ({
      page,
    }) => {
      const { posted } = await setup(page);
      const input = await open(page);
      await expect(input).toBeVisible();
      const container = input.locator('xpath=..');
      const send = container.locator('button[title="Send comment (Enter)"]');

      const oneLine = await box(container);
      expect(Math.round(oneLine.height)).toBeLessThanOrEqual(40);
      const sendOneLine = await box(send);
      const inputOneLine = await box(input);

      await input.fill('first line');
      await input.press('Shift+Enter');
      await input.pressSequentially('second line');
      await expect(input).toHaveValue('first line\nsecond line');
      expect(posted).toHaveLength(0);
      const twoLines = await box(input);
      expect(twoLines.height).toBeGreaterThanOrEqual(inputOneLine.height + 19);

      await input.fill(
        Array.from({ length: 20 }, (_, i) => `line ${i}`).join('\n'),
      );
      const capped = await input.evaluate((el) => ({
        h: el.getBoundingClientRect().height,
        scroll: el.scrollHeight,
        client: el.clientHeight,
      }));
      expect(capped.h).toBeLessThanOrEqual(128.5);
      expect(capped.h).toBeGreaterThan(120);
      expect(capped.scroll).toBeGreaterThan(capped.client);
      const grown = await box(container);
      const sendGrown = await box(send);
      expect(
        Math.abs(
          grown.bottom -
            sendGrown.bottom -
            (oneLine.bottom - sendOneLine.bottom),
        ),
      ).toBeLessThanOrEqual(1);

      await input.fill('a long comment '.repeat(40));
      await input.press('Enter');
      await expect.poll(() => posted.length).toBe(1);
      expect(posted[0].message).toBe('a long comment '.repeat(40).trim());
      await expect(input).toHaveValue('');
      await expect(input).toBeFocused();
      await expect
        .poll(async () => (await box(container)).height)
        .toBeCloseTo(oneLine.height, 0);

      await input.fill('sent with the button');
      await send.click();
      await expect.poll(() => posted.length).toBe(2);
      await expect(input).toBeFocused();
    });

    test('Esc blurs a field with a draft and keeps it', async ({ page }) => {
      await setup(page);
      const input = await open(page);
      const form = page.locator('.new-form-container');
      await input.fill('draft\nsecond line');
      const grown = (await box(input)).height;
      await input.press('Escape');
      await expect(input).not.toBeFocused();
      await expect(input).toHaveValue('draft\nsecond line');
      await expect(form).toBeVisible();
      expect((await box(input)).height).toBeCloseTo(grown, 0);

      if (name === 'modal') {
        await page.keyboard.press('Escape');
        await expect(form).toHaveCount(0);
      }
    });

    if (name === 'modal') {
      test('Esc in an empty focused field closes the modal', async ({
        page,
      }) => {
        await setup(page);
        const input = await open(page);
        await input.focus();
        await input.press('Escape');
        await expect(page.locator('.new-form-container')).toHaveCount(0);
      });
    }

    test('field height follows a window resize', async ({ page }) => {
      await setup(page);
      const input = await open(page);
      await input.fill('a long comment that wraps '.repeat(3));
      const before = (await box(input)).height;
      // The modal field gets narrower at 1100px (one more line); the page rail goes full width
      // below lg (one line less).
      await page.setViewportSize({
        width: name === 'modal' ? 1100 : 800,
        height: 900,
      });
      await expect
        .poll(async () => Math.round((await box(input)).height))
        .not.toBe(Math.round(before));
      expect(
        await input.evaluate((el) => el.scrollHeight - el.clientHeight),
      ).toBeLessThanOrEqual(0);
    });

    test('a blank field does not send', async ({ page }) => {
      const { posted } = await setup(page);
      const input = await open(page);
      const send = input
        .locator('xpath=..')
        .locator('button[title="Send comment (Enter)"]');
      await input.fill('   ');
      await input.press('Enter');
      await expect(send).toBeDisabled();
      await page.waitForTimeout(200);
      expect(posted).toHaveLength(0);
    });

    test('Enter that commits an IME composition does not send', async ({
      page,
    }) => {
      const { posted } = await setup(page);
      const input = await open(page);
      await input.fill('こんにちは');
      await input.evaluate((el) => {
        el.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'Enter',
            isComposing: true,
            bubbles: true,
          }),
        );
        const webkit = new KeyboardEvent('keydown', {
          key: 'Enter',
          bubbles: true,
        });
        Object.defineProperty(webkit, 'keyCode', { get: () => 229 });
        el.dispatchEvent(webkit);
      });
      await page.waitForTimeout(200);
      expect(posted).toHaveLength(0);
      await expect(input).toHaveValue('こんにちは');
      await input.press('Enter');
      await expect.poll(() => posted.length).toBe(1);
    });

    test('AI step chips stay in one scrolling row', async ({ page }) => {
      const { asked } = await setup(page, { aiReply: '1' });
      const input = await open(page);
      await input.fill('what is going on?');
      await input.press('Enter');
      await expect.poll(() => asked.length).toBe(1);
      await expect(page.getByText('AI is looking around…')).toBeVisible();
      const row = page.getByText(/^AI is looking around/).locator('xpath=..');

      await input.evaluate((el) => {
        let inst = el.__vueParentComponent;
        while (inst && !('aiPendingSteps' in (inst.setupState ?? {})))
          inst = inst.parent;
        inst.setupState.aiPendingSteps = Array.from({ length: 8 }, (_, i) => ({
          seq: i + 1,
          tool: `reading_a_rather_long_tool_name_number_${i + 1}`,
        }));
      });
      await expect(row.locator('span.rounded-pill')).toHaveCount(8);
      const m = await row.evaluate((el) => {
        const chip = el.querySelector('span.rounded-pill');
        const surface = el.closest('footer') ?? el.closest('aside');
        return {
          row: el.getBoundingClientRect().height,
          chip: chip.getBoundingClientRect().height,
          chipLines: chip.getClientRects().length,
          scroll: el.scrollWidth,
          client: el.clientWidth,
          surfaceOverflow: surface.scrollWidth - surface.clientWidth,
          docOverflow:
            document.documentElement.scrollWidth -
            document.documentElement.clientWidth,
        };
      });
      expect(m.row).toBeCloseTo(m.chip, 0);
      expect(m.chipLines).toBe(1);
      expect(m.scroll).toBeGreaterThan(m.client);
      expect(m.surfaceOverflow).toBeLessThanOrEqual(0);
      expect(m.docOverflow).toBeLessThanOrEqual(0);
    });
  });
}
