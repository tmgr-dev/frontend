import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const E2E = process.env.ONBOARDING_E2E === '1';
const flow = E2E ? test : test.skip;
const SHOTS =
  process.env.ONBOARDING_SHOTS ??
  `${process.env.HOME}/projects/own-projects/tmgr.dev/docs/reports/onboarding/s3`;

const SIZES = [
  { name: '1440', width: 1440, height: 900 },
  { name: '1000', width: 1000, height: 800 },
  { name: 'phone', width: 390, height: 844 },
];
const SCHEMES = ['light', 'dark'];
const NEW_USER = { created_at: new Date().toISOString() };

const setScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.evaluate((dark) => document.documentElement.classList.toggle('dark', dark), scheme === 'dark');
  await page.waitForTimeout(300);
};

const shoot = async (page, name, size, scheme) => {
  await setScheme(page, scheme);
  await page.screenshot({ path: `${SHOTS}/${name.startsWith('fu-') ? '' : 'fix-'}${name}-${size.name}-${scheme}.png` });
};

const boot = async (page, size, scheme) => {
  await page.setViewportSize({ width: size.width, height: size.height });
  await page.emulateMedia({ colorScheme: scheme });
  const app = await mockApp(page, {
    multiStatus: true,
    onboarding: { tips_mode: 'auto', intent: 'agent' },
    userExtra: NEW_USER,
  });
  await page.route('**/api/smart-device/token/**', (route) =>
    route.fulfill({ json: { data: { token: 'fixture-token', has_token: false } } }),
  );
  return app.onboarding;
};

const connect = async (fx, page) => {
  fx.state.facts.mcp_first_call_at = true;
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await expect(page.getByTestId('starter-prompt')).toBeVisible({ timeout: 20000 });
};

const intersects = (a, b) =>
  !(a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y);

const expectCopyClear = async (page) => {
  const texts = page.getByTestId('snippet-text');
  const copies = page.getByTestId('snippet-copy');
  const count = await texts.count();
  expect(count).toBeGreaterThan(0);
  for (let i = 0; i < count; i++) {
    const text = await texts.nth(i).boundingBox();
    const copy = await copies.nth(i).boundingBox();
    expect(intersects(text, copy)).toBe(false);
  }
};

const expectCardsInViewport = async (page) => {
  const boxes = await page.evaluate(() =>
    [...document.querySelectorAll('.tour-host, .driver-popover.tour-driver-popover, [role="dialog"][aria-labelledby^="tour-"]')]
      .filter((el) => el.getClientRects().length)
      .map((el) => {
        const r = el.getBoundingClientRect();
        return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, w: innerWidth, h: innerHeight };
      }),
  );
  expect(boxes.length).toBeGreaterThan(0);
  for (const b of boxes) {
    expect(b.left).toBeGreaterThanOrEqual(-0.5);
    expect(b.top).toBeGreaterThanOrEqual(-0.5);
    expect(b.right).toBeLessThanOrEqual(b.w + 0.5);
    expect(b.bottom).toBeLessThanOrEqual(b.h + 0.5);
  }
};

const expectClearOfCtas = async (page, label, shotName = null) => {
  await page.waitForTimeout(2000);
  if (shotName) await shoot(page, shotName.name, shotName.size, shotName.scheme);
  const { cards, ctas, vh } = await page.evaluate(() => {
    const box = (el) => {
      const r = el.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    };
    const cards = [...document.querySelectorAll('.tour-host, .driver-popover.tour-driver-popover')]
      .filter((el) => el.getClientRects().length && el.querySelector('[role="dialog"]'))
      .map(box);
    const ctas = [...document.querySelectorAll('[data-tour-avoid]')]
      .filter((el) => el.getClientRects().length)
      .map((el) => ({ text: el.textContent.trim(), ...box(el) }));
    return { cards, ctas, vh: innerHeight };
  });
  expect(cards.length, `${label}: card`).toBeGreaterThan(0);
  for (const cta of ctas) {
    for (const c of cards) {
      expect(intersects(cta, c), `${label}: card covers "${cta.text}"`).toBe(false);
    }
  }
  return ctas.length;
};

const startTour = async (page) => {
  await page.goto('/demo/getting-started');
  await page.getByTestId('hub-start-start.connect-agent').click();
  await expect(page).toHaveURL(/\/welcome-setup\/agent$/);
};

const card = (page) => page.locator('[role="dialog"][aria-labelledby^="tour-"]').first();

for (const size of SIZES) {
  for (const scheme of SCHEMES) {
    flow(`connect layout: ${size.name} ${scheme}`, async ({ page }) => {
      const fx = await boot(page, size, scheme);
      await startTour(page);
      await expect(page.getByTestId('shown-once')).toBeVisible();
      await expectCopyClear(page);
      for (const tab of ['Codex', 'Other (JSON)']) {
        await page.getByRole('tab', { name: tab }).click();
        await expectCopyClear(page);
      }
      await page.getByRole('tab', { name: 'Claude Code' }).click();
      await expect(card(page)).toBeVisible();
      await expectCardsInViewport(page);
      await shoot(page, 'connect-cli', size, scheme);

      await connect(fx, page);
      const prompt = page.getByTestId('starter-prompt').getByTestId('snippet-text');
      const fits = await prompt.evaluate((el) => el.scrollWidth <= el.clientWidth);
      expect(fits).toBe(true);
      const promptText = await prompt.evaluate((el) => el.textContent);
      expect(promptText).toContain('then finish the agent work.');
      const text = await prompt.boundingBox();
      const copy = await page.getByTestId('starter-prompt').getByTestId('snippet-copy').boundingBox();
      expect(intersects(text, copy)).toBe(false);
      if (size.width < 640) expect(copy.height).toBeGreaterThanOrEqual(44);
      await page.waitForTimeout(2500);
      if (await card(page).isVisible()) await expectCardsInViewport(page);
      await shoot(page, 'connected-prompt', size, scheme);
    });

    flow(`connect tour is not stuck on the copy step once connected: ${size.name} ${scheme}`, async ({ page }) => {
      const fx = await boot(page, size, scheme);
      await startTour(page);
      await expect(card(page)).toBeVisible();
      await expect(card(page)).toContainText('Pick your client');
      await card(page).getByRole('button', { name: 'Next' }).click();
      await expect(card(page)).toContainText('Copy the command');
      await expectCardsInViewport(page);
      await shoot(page, 'tour-copy-step', size, scheme);
      await connect(fx, page);
      await expect(card(page)).not.toContainText('Copy the command');
      await expect(card(page)).toContainText('Try a prompt', { timeout: 10000 });
      await expectCardsInViewport(page);
      await shoot(page, 'tour-prompt-step', size, scheme);
    });
  }
}

flow('tour card with a missing anchor stays inside a short viewport', async ({ page }) => {
  const size = { name: 'short', width: 360, height: 300 };
  const fx = await boot(page, size, 'light');
  await startTour(page);
  await expect(card(page)).toBeVisible();
  await card(page).getByRole('button', { name: 'Next' }).click();
  await connect(fx, page);
  await expectCardsInViewport(page);
});

for (const size of SIZES) {
  for (const scheme of SCHEMES) {
    flow(`tour card never covers a primary CTA: ${size.name} ${scheme}`, async ({ page }) => {
      await page.clock.install();
      const fx = await boot(page, size, scheme);
      await startTour(page);
      await expect(card(page)).toContainText('Pick your client');
      let seen = await expectClearOfCtas(page, 's1 not connected', { name: 'fu-avoid-s1', size, scheme });
      await expectCardsInViewport(page);
      await card(page).getByRole('button', { name: 'Next' }).click();
      await expect(card(page)).toContainText('Copy the command');
      seen += await expectClearOfCtas(page, 's2 not connected', { name: 'fu-avoid-s2', size, scheme });
      await expectCardsInViewport(page);
      await card(page).getByRole('button', { name: /Skip (this )?step/ }).click();
      await expect(card(page)).toContainText('Waiting for your agent');
      seen += await expectClearOfCtas(page, 's3 waiting', { name: 'fu-avoid-s3', size, scheme });
      await page.clock.fastForward(181000);
      await expect(page.getByTestId('still-waiting')).toBeVisible();
      await expect(card(page)).toBeVisible();
      seen += await expectClearOfCtas(page, 's3 still waiting', { name: 'fu-avoid-still-waiting', size, scheme });
      await expectCardsInViewport(page);
      expect(seen).toBeGreaterThan(0);
    });

    flow(`tour card never covers Open my workspace once connected: ${size.name} ${scheme}`, async ({ page }) => {
      const fx = await boot(page, size, scheme);
      await startTour(page);
      await expect(card(page)).toBeVisible();
      await connect(fx, page);
      await expect(card(page)).toContainText('Try a prompt', { timeout: 10000 });
      const seen = await expectClearOfCtas(page, 's4 connected', { name: 'fu-avoid-s4-connected', size, scheme });
      expect(seen).toBeGreaterThan(0);
      const cta = page.getByRole('button', { name: 'Open my workspace' });
      await expect(cta).toBeVisible();
      await expectCardsInViewport(page);
      const vp = page.viewportSize();
      const box = await cta.boundingBox();
      expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
      expect(box.y).toBeGreaterThanOrEqual(0);
    });
  }
}
