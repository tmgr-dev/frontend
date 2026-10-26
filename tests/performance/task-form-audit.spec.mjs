import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

const LONG_URL = `https://example.com/${'a'.repeat(160)}`;
const LONG_CODE = 'x'.repeat(120);
const DESCRIPTION = [
  'We ship feature flags so every team can toggle. Please keep the rollout notes short and readable on small screens.',
  '',
  `Link: ${LONG_URL}`,
  '',
  `Inline \`${LONG_CODE}\` code stays inside the column.`,
].join('\n');

const baseTask = {
  id: 1,
  title: 'Original task',
  description: DESCRIPTION,
  description_json: null,
  status: 'created',
  status_id: 1,
  workspace_id: 1,
  user_id: 1,
  user: { id: 1, name: 'Test User', email: 'test@example.test' },
  assignees: [],
  settings: [],
  checkpoints: [{ start: 0, end: 600, description: 'Did the work' }],
  common_time: 0,
  start_time: 0,
  approximately_time: 0,
  project_category_id: 7,
  relationTypeWithTask: [
    {
      id: 11,
      relation_type: { id: 1, name: 'relates to' },
      related_task: {
        id: 2,
        title: 'Other task',
        status_id: 1,
        workspace_id: 1,
      },
    },
  ],
  created_at: '2026-09-13T10:00:00Z',
  updated_at: '2026-09-13T10:00:00Z',
};

const comment = {
  id: 5,
  message: 'First comment',
  created_at: '2026-09-13T10:00:00Z',
  user: { id: 1, name: 'Test User', email: 'test@example.test' },
};

const setup = async (
  page,
  { colorScheme = 'light', deleteStatus = 200 } = {},
) => {
  await page.addInitScript((scheme) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('preferred_editor', 'markdown');
    localStorage.setItem('colorScheme', scheme);
  }, colorScheme);
  await mockApp(page);
  const deletes = [];
  await page.route('**/api/tasks/1', (route) => {
    const method = route.request().method();
    if (method === 'DELETE') {
      deletes.push(method);
      return route.fulfill(
        deleteStatus === 200
          ? { json: { data: {} } }
          : { status: deleteStatus, json: { message: 'Fixture failure' } },
      );
    }
    return route.fulfill({ json: { data: baseTask } });
  });
  await page.route('**/api/tasks/1/comments**', (route) =>
    route.fulfill({ json: { data: [comment] } }),
  );
  await page.route('**/api/task-relation-types', (route) =>
    route.fulfill({
      json: { data: [{ id: 1, name: 'relates to' }] },
    }),
  );
  await page.route('**/api/tasks/search**', (route) =>
    route.fulfill({
      json: [{ id: 2, title: 'Other task', status_id: 1, workspace_id: 1 }],
    }),
  );
  return { deletes };
};

const openModal = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByLabel('Task name')).toHaveValue('Original task');
  return form;
};

const openPage = async (page) => {
  await page.goto('/demo/tasks/1');
  const form = page.locator('.new-form-container');
  await expect(form.getByLabel('Task name')).toHaveValue('Original task');
  return form;
};

// Words that fit on a line must never be split across two lines.
const splitWords = (page) =>
  page.evaluate(() => {
    const root = document.querySelector('.md-editor-preview');
    const split = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const re = /\S+/g;
      let m;
      while ((m = re.exec(node.data))) {
        if (m[0].length > 20) continue;
        const range = document.createRange();
        range.setStart(node, m.index);
        range.setEnd(node, m.index + m[0].length);
        const tops = new Set(
          [...range.getClientRects()].map((r) => Math.round(r.top)),
        );
        if (tops.size > 1) split.push(m[0]);
      }
    }
    return {
      split,
      scrollWidth: root.scrollWidth,
      clientWidth: root.clientWidth,
      wordBreak: getComputedStyle(root).wordBreak,
      overflowWrap: getComputedStyle(root).overflowWrap,
    };
  });

for (const mode of ['modal', 'page']) {
  test(`description wraps at word boundaries at 390 px (${mode})`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await setup(page);
    if (mode === 'modal') await openModal(page);
    else await openPage(page);
    await expect(page.locator('.md-editor-preview')).toContainText(
      'feature flags',
    );
    const result = await splitWords(page);
    expect(result.wordBreak).toBe('normal');
    expect(result.overflowWrap).toBe('anywhere');
    expect(result.split).toEqual([]);
    expect(result.scrollWidth).toBeLessThanOrEqual(result.clientWidth);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(390);
  });
}

test('the "New task in this category" button shows its plus icon', async ({
  page,
}) => {
  await setup(page);
  const form = await openModal(page);
  const icon = form
    .locator('button[title^="New task in"]')
    .first()
    .locator('svg');
  await expect(icon).toBeVisible();
  const box = await icon.boundingBox();
  expect(box.width).toBeGreaterThan(0);
  expect(box.height).toBeGreaterThan(0);
});

for (const mode of ['modal', 'page']) {
  test.describe(`delete (${mode})`, () => {
    const open = (page) =>
      mode === 'modal' ? openModal(page) : openPage(page);

    test('Cancel keeps the task, OK deletes it', async ({ page }) => {
      const { deletes } = await setup(page);
      const form = await open(page);
      const confirm = page.getByText('Are you sure you want to delete');

      await form.locator('footer').getByLabel('Delete').click();
      await expect(
        page.getByText('Are you sure you want to delete "Original task"?'),
      ).toBeVisible();
      await page.getByRole('button', { name: 'Cancel' }).click();
      await expect(confirm).toHaveCount(0);
      await expect(form.getByLabel('Task name')).toBeVisible();
      expect(deletes).toEqual([]);

      await form.locator('footer').getByLabel('Delete').click();
      await page.getByRole('button', { name: 'Ok' }).click();
      await expect.poll(() => deletes.length).toBe(1);
      if (mode === 'modal')
        await expect(page.locator('.new-form-container')).toHaveCount(0);
      else await expect(page).not.toHaveURL(/\/tasks\/1$/);
    });

    test('Escape and the backdrop close only the confirm', async ({ page }) => {
      const { deletes } = await setup(page);
      const form = await open(page);
      await form.locator('footer').getByLabel('Delete').click();
      const body = page.getByText('Are you sure you want to delete');
      await expect(body).toBeVisible();
      await page.mouse.click(4, 4);
      await expect(body).toHaveCount(0);
      await expect(form.getByLabel('Task name')).toBeVisible();
      await form.locator('footer').getByLabel('Delete').click();
      await expect(body).toBeVisible();
      await page.keyboard.press('Escape');
      await expect(body).toHaveCount(0);
      await expect(form.getByLabel('Task name')).toBeVisible();
      expect(deletes).toEqual([]);
    });

    test('a failed delete shows an error toast and keeps the task', async ({
      page,
    }) => {
      const { deletes } = await setup(page, { deleteStatus: 500 });
      const form = await open(page);
      await form.locator('footer').getByLabel('Delete').click();
      await page.getByRole('button', { name: 'Ok' }).click();
      await expect(
        page.getByText("Couldn't delete the task", { exact: true }).first(),
      ).toBeVisible();
      expect(deletes.length).toBe(1);
      await expect(form.getByLabel('Task name')).toBeVisible();
      await expect(form.locator('footer').getByLabel('Save')).toHaveAttribute(
        'aria-busy',
        'false',
      );
    });
  });
}

test('the comments rail sits under the task below 1280 px and beside it at 1440 px', async ({
  page,
}) => {
  await setup(page);
  const geometry = async () => {
    const panel = await page
      .locator('.new-form-container footer')
      .boundingBox();
    const rail = await page
      .locator('aside')
      .filter({ hasText: 'Comments' })
      .last()
      .boundingBox();
    return { panel, rail };
  };

  await page.setViewportSize({ width: 1024, height: 800 });
  await openPage(page);
  let { panel, rail } = await geometry();
  expect(rail.y).toBeGreaterThanOrEqual(panel.y + panel.height - 1);
  expect(rail.width).toBeGreaterThan(700);

  await page.setViewportSize({ width: 1440, height: 900 });
  ({ panel, rail } = await geometry());
  expect(rail.x).toBeGreaterThan(panel.x + panel.width - 2);
  expect(rail.width).toBeLessThan(500);
});

test('icon-only buttons have names', async ({ page }) => {
  await setup(page);
  const form = await openPage(page);
  await expect(
    form.getByRole('button', { name: 'Delete comment' }),
  ).toHaveCount(1);
  await expect(
    form.getByRole('button', { name: 'Remove relation' }),
  ).toHaveCount(1);
  await expect(form.getByRole('button', { name: 'Delete entry' })).toHaveCount(
    1,
  );
  await form.locator('button:has(> span:text-is("Link"))').click();
  await expect(page.getByRole('button', { name: 'Close' })).toBeVisible();
  await expect(form.getByLabel('Task name')).toHaveAttribute(
    'aria-label',
    'Task name',
  );
});

test('upload dismiss button is named', async ({ page }) => {
  await setup(page);
  const form = await openPage(page);
  const chooser = page.waitForEvent('filechooser');
  await form.getByRole('button', { name: 'Add files' }).click();
  await (
    await chooser
  ).setFiles({
    name: 'empty.txt',
    mimeType: 'text/plain',
    buffer: Buffer.alloc(0),
  });
  await expect(
    form.getByRole('button', { name: 'Dismiss upload' }),
  ).toBeVisible();
});

const opacityOf = (locator) =>
  locator.evaluate((el) => {
    let value = 1;
    for (let node = el; node; node = node.parentElement)
      value *= Number(getComputedStyle(node).opacity);
    return value;
  });

test('hover-only actions appear on keyboard focus', async ({ page }) => {
  await setup(page);
  const form = await openPage(page);
  const targets = [
    form.getByRole('button', { name: 'Delete comment' }),
    form.getByRole('button', { name: 'Remove relation' }),
    form.getByRole('button', { name: 'Delete entry' }),
  ];
  for (const target of targets) {
    await expect.poll(() => opacityOf(target)).toBe(0);
    await target.focus();
    await expect.poll(() => opacityOf(target)).toBe(1);
    await target.blur();
  }
});

test.describe('touch', () => {
  test.use({ hasTouch: true, isMobile: true });

  test('hover-only actions are visible without hover', async ({ page }) => {
    await setup(page);
    const form = await openPage(page);
    for (const name of ['Delete comment', 'Remove relation', 'Delete entry'])
      await expect
        .poll(() => opacityOf(form.getByRole('button', { name })))
        .toBe(1);
  });
});

const indicator = (locator) =>
  locator.evaluate((el) => {
    const s = getComputedStyle(el);
    return [
      s.outlineStyle === 'none'
        ? 'none'
        : `${s.outlineWidth} ${s.outlineColor}`,
      s.boxShadow,
      s.borderColor,
      s.backgroundColor,
    ].join('|');
  });

for (const scheme of ['light', 'dark']) {
  test(`title and relation fields show a focus indicator (${scheme})`, async ({
    page,
  }) => {
    await setup(page, { colorScheme: scheme });
    const form = await openPage(page);

    const title = form.getByLabel('Task name');
    await title.blur();
    const idleTitle = await indicator(title);
    await page.keyboard.press('Shift+Tab');
    await title.focus();
    await expect.poll(() => indicator(title)).not.toBe(idleTitle);
    expect(
      await title.evaluate((el) => getComputedStyle(el).borderBottomWidth),
    ).toBe('2px');
    expect(
      await title.evaluate((el) => el.scrollHeight <= el.clientHeight),
    ).toBe(true);

    await form.locator('button:has(> span:text-is("Link"))').click();
    const search = page.getByPlaceholder('Search…');
    await search.fill('Oth');
    const result = page.getByRole('button', { name: 'Other task' }).last();
    await expect(result).toBeVisible();
    await page.keyboard.press('Tab');
    await result.blur();
    const idle = await indicator(result);
    await result.focus();
    await expect.poll(() => indicator(result)).not.toBe(idle);
    await search.blur();
    const idleSearch = await indicator(search);
    await search.focus();
    await expect.poll(() => indicator(search)).not.toBe(idleSearch);
  });
}
