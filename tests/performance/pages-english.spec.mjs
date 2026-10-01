import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';

const shotDir = process.env.PAGES_SHOTS;
if (shotDir) mkdirSync(shotDir, { recursive: true });

const shot = async (page, name) => {
  if (!shotDir) return;
  await page.emulateMedia({ colorScheme: 'light' });
  await page.evaluate(() => document.documentElement.classList.remove('dark'));
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${shotDir}/${name}.png` });
};

const CYRILLIC = /[Ѐ-ӿ]/;
const author = { kind: 'user', id: 1, name: 'Test User' };
const summary = (id, title, slug, type, extra = {}) => ({
  id,
  title,
  slug,
  type,
  parent_id: null,
  position: id,
  pinned: false,
  updated_at: '2026-10-01T10:00:00Z',
  ...extra,
});

const personBody = [
  '## Summary',
  '',
  '<!-- tmgr:section id="promises" owner="system" -->',
  '## Promises',
  '- [TM-1](tmgr://task/1) - Send the report',
  '<!-- /tmgr:section -->',
  '',
  '## Timeline',
  '',
  '## What I know',
  '',
  '## Insights',
  '',
].join('\n');

const meetingBody = [
  '## Agenda',
  '',
  '## Outcomes',
  '',
  '## Decisions',
  '',
  '## Action items',
  '- Call the vendor',
  '',
].join('\n');

const pages = [
  summary(1, 'Ann Lee', 'ann-lee', 'person'),
  summary(2, 'Weekly sync', 'weekly-sync', 'meeting', { pinned: true }),
  summary(3, 'Workspace context', 'workspace-context', 'context'),
];

const full = (summaryRow, body, properties = {}) => ({
  ...summaryRow,
  workspace_id: 1,
  body,
  properties,
  version: 3,
  author,
  updated_by: author,
  created_at: '2026-09-30T10:00:00Z',
  backlinks: [pages[1]],
  sections: [],
  files: [],
  following: false,
});

const versions = [3, 2, 1].map((version) => ({
  version,
  title: 'Ann Lee',
  author,
  summary: version === 2 ? 'Promises updated' : null,
  created_at: `2026-10-0${version}T10:00:00Z`,
}));

const setup = async (page) => {
  await mockApp(page);
  await page.route('**/api/**/feature-toggles', (route) => {
    const toggle = (key) => ({
      key,
      name: key,
      group: 'pages',
      type: 'boolean',
      enabled: true,
    });
    return route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {
              default_landing_page: {
                key: 'default_landing_page',
                name: 'Default Landing Page',
                group: 'general',
                type: 'select',
                options: ['list', 'board', 'dashboard', 'daily_routines'],
                value: 'list',
              },
            }
          : { board: toggle('board'), pages: toggle('pages') },
      },
    });
  });
  await page.route('**/api/pages**', (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/pages', '').replace(/^\//, '');
    const json = (data) => route.fulfill({ json: { data } });
    if (path === 'tree' || path === '') return json(pages);
    if (path === 'trash')
      return json([
        { ...pages[2], deleted_at: '2026-10-01T09:00:00Z', title: 'Old notes' },
      ]);
    if (path === 'search')
      return json([
        {
          id: 1,
          slug: 'ann-lee',
          title: 'Ann Lee',
          type: 'person',
          snippet: 'Promised the report',
          updated_at: '2026-10-01T10:00:00Z',
        },
      ]);
    if (path === 'ann-lee')
      return json(
        full(pages[0], personBody, {
          network: 'operational',
          company: 'Acme',
          role: 'CTO',
          aliases: [{ source: 'email', native_id: 'ann@example.test', display: 'Ann' }],
          last_contact_at: '2026-10-01',
        }),
      );
    if (path === 'weekly-sync')
      return json(
        full(pages[1], meetingBody, {
          date: '2026-10-01',
          participants: [],
          related_tasks: [],
        }),
      );
    if (path === 'ann-lee/versions') return json(versions);
    if (path.startsWith('ann-lee/versions/'))
      return json({ ...versions[0], body: personBody, properties: {} });
    return json([]);
  });
  await page.route('**/api/workspaces/1/categories*', (route) =>
    route.fulfill({ json: { data: [] } }),
  );
};

const visibleText = (page) => page.locator('body').innerText();

test.describe('Pages UI is English', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    await setup(page);
  });

  test('sidebar and index', async ({ page }) => {
    await page.goto('/demo/pages');
    await expect(page.getByText('Pages').first()).toBeVisible();
    await expect(page.getByText('Create page').first()).toBeVisible();
    await expect(page.getByText('Trash').first()).toBeVisible();
    await expect(page.getByText('All pages').first()).toBeVisible();
    await shot(page, 'index');
    expect(await visibleText(page)).not.toMatch(CYRILLIC);
  });

  test('person page with properties and side panel', async ({ page }) => {
    await page.goto('/demo/pages/ann-lee');
    await expect(page.getByText('Backlinks')).toBeVisible();
    await expect(page.getByText('Contents')).toBeVisible();
    await expect(page.getByText('Versions').first()).toBeVisible();
    await expect(page.locator('option[value="operational"]').first()).toHaveText(
      'Operational',
    );
    await expect(page.getByText('Last contact')).toBeVisible();
    await expect(page.getByText('Add alias')).toBeVisible();
    await shot(page, 'person');
    expect(await visibleText(page)).not.toMatch(CYRILLIC);
  });

  test('meeting page offers Make a task', async ({ page }) => {
    await page.goto('/demo/pages/weekly-sync');
    await expect(page.getByTestId('page-action-lines')).toContainText(
      'Action items',
    );
    await page.getByTestId('action-to-task').click();
    await expect(page.getByText('Make a task').first()).toBeVisible();
    await expect(page.getByText('Create task')).toBeVisible();
    await shot(page, 'meeting');
    expect(await visibleText(page)).not.toMatch(CYRILLIC);
  });

  test('version history', async ({ page }) => {
    await page.goto('/demo/pages/ann-lee/versions');
    await expect(page.getByText('History: Ann Lee')).toBeVisible();
    await expect(page.getByText('Restore').first()).toBeVisible();
    await shot(page, 'versions');
    expect(await visibleText(page)).not.toMatch(CYRILLIC);
  });

  test('trash', async ({ page }) => {
    await page.goto('/demo/pages/_/trash');
    await expect(page.getByRole('heading', { name: 'Pages trash' })).toBeVisible();
    await expect(page.getByText('Restore').first()).toBeVisible();
    await shot(page, 'trash');
    expect(await visibleText(page)).not.toMatch(CYRILLIC);
  });

  test('global search Pages tab', async ({ page }) => {
    await page.goto('/demo/pages');
    await page.getByRole('button', { name: 'Search', exact: true }).first().click();
    await page.getByRole('tab', { name: 'Pages', exact: true }).click();
    await page.getByPlaceholder('Page title or text').fill('ann');
    await expect(page.getByText('Ann Lee').first()).toBeVisible();
    await shot(page, 'search');
    expect(await visibleText(page)).not.toMatch(CYRILLIC);
  });
});
