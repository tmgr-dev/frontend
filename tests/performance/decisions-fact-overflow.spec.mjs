import { expect, test } from '@playwright/test';

const LONG = 'a'.repeat(240);

const card = (over) => ({
  id: 'c_1',
  created_ms: 1000,
  updated_ms: 1000,
  state: 'open',
  session_gone: false,
  can_reply: true,
  listening: false,
  messages: [],
  source: { agent: 'claude-code' },
  kind: 'decision',
  blocking: false,
  title: 'Overflow check',
  facts: [],
  options: [],
  checks: [],
  diffs: [],
  patches: [],
  links: {},
  reply_via: 'mcp',
  ...over,
});

const openDecisions = async (page, cards) => {
  await page.addInitScript((list) => {
    let current = list;
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: (cmd, args) => {
        if (cmd === 'decisions_list') return Promise.resolve(current);
        if (cmd === 'decisions_answer')
          current = current.map((c) =>
            c.id === args.id
              ? { ...c, state: 'answered', answer: args.answer }
              : c,
          );
        return Promise.resolve(null);
      },
      transformCallback: () => 0,
    };
  }, cards);
  await page.goto('/decisions.html');
  await page.locator('.card').waitFor();
};

const measure = (page) =>
  page.evaluate(() => {
    const card = document.querySelector('.card');
    const style = getComputedStyle(card);
    const right = Math.max(
      ...[
        ...card.querySelectorAll(
          '.q, .fact, .fact > span, .chip, .notice, .notice > div',
        ),
      ].map((el) => el.getBoundingClientRect().right),
    );
    return {
      scrollWidth: card.scrollWidth,
      clientWidth: card.clientWidth,
      contentRight:
        card.getBoundingClientRect().right - parseFloat(style.paddingRight),
      right,
    };
  });

test('a long unbroken fact, question and plain chip wrap inside the card', async ({
  page,
}) => {
  await openDecisions(page, [
    card({
      question: LONG,
      facts: [{ state: 'ok', text: LONG }],
      links: {
        items: [{ kind: 'url', url: 'https://example.com/x', label: LONG }],
      },
    }),
  ]);

  const m = await measure(page);
  expect(m.scrollWidth).toBeLessThanOrEqual(m.clientWidth);
  expect(m.right).toBeLessThanOrEqual(m.contentRight + 0.5);
});

test('a long unbroken final detail wraps inside the card', async ({ page }) => {
  await openDecisions(page, [
    card({ options: [{ id: 'a', label: 'A', recommended: false }] }),
  ]);
  await page.locator('.opt .add').click();
  await page.locator('[data-input="note"]').fill(LONG);
  await page.keyboard.press('Enter');
  await expect(page.locator('.notice.final')).toBeVisible();

  const m = await measure(page);
  expect(m.scrollWidth).toBeLessThanOrEqual(m.clientWidth);
  expect(m.right).toBeLessThanOrEqual(m.contentRight + 0.5);
});
