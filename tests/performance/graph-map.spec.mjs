import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { buildGraphMap } from './graphMapFixture.mjs';
import { debugInfo, openMap, setScheme, setupMap } from './graphMapHelpers.mjs';

const shotDir = process.env.GRAPH_SHOTS;
if (shotDir) mkdirSync(shotDir, { recursive: true });
const shot = async (page, name) => {
  if (shotDir) await page.screenshot({ path: `${shotDir}/${name}.png` });
};

const map = buildGraphMap();

test.describe('Workspace map', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
  });

  test('renders clusters, insights and the legend in both themes', async ({
    page,
  }) => {
    const requests = await setupMap(page, map);
    await page.emulateMedia({ colorScheme: 'dark' });
    const canvas = await openMap(page);
    await setScheme(page, 'dark');
    await expect(
      page.getByRole('heading', { name: /Map — how everything connects/ }),
    ).toBeVisible();
    expect(requests[0].workspace_id).toBe('1');
    expect(requests[0].limit).toBe('3000');
    expect(requests[0].from).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    const insights = page.getByTestId('map-insights');
    await expect(insights.getByTestId('insight-bottleneck')).toContainText(
      'TM-416 blocks 4 tasks in',
    );
    await expect(insights.getByTestId('insight-bottleneck')).toContainText(
      'Desktop',
    );
    await expect(insights.getByTestId('insight-hubs')).toContainText(
      'tie the most items together',
    );
    await expect(insights.getByTestId('insight-bridges')).toContainText(
      'connect',
    );
    await expect(insights.getByTestId('insight-orphans')).toContainText(
      `${map.insights.orphans.total} items`,
    );
    await expect(page.getByTestId('map-legend')).toContainText(
      'Orange = bottleneck',
    );
    const info = await debugInfo(canvas);
    expect(info.clusters.length).toBe(6);
    expect(info.bottleneck).not.toBeNull();
    await shot(page, 'map-dark-1440');
    await setScheme(page, 'light');
    await shot(page, 'map-light-1440');
  });

  test('clicking a cluster zooms in and clicking a hub opens the neighbourhood', async ({
    page,
  }) => {
    await setupMap(page, map);
    await page.emulateMedia({ colorScheme: 'dark' });
    const canvas = await openMap(page);
    await setScheme(page, 'dark');
    const before = await debugInfo(canvas);
    const target = before.clusters.find((c) => c.title === 'Desktop');
    const box = await canvas.boundingBox();
    await page.mouse.click(box.x + target.x, box.y + target.top);
    await page.waitForTimeout(1100);
    const after = await debugInfo(canvas);
    expect(after.k).toBeGreaterThan(before.k * 1.4);
    await shot(page, 'map-cluster-zoom-dark');
    const hub = after.bottleneck;
    await page.mouse.click(box.x + hub.x, box.y + hub.y);
    await expect(page.getByTestId('graph-overlay')).toBeVisible();
  });

  test('search highlights matches and Enter zooms to the first one', async ({
    page,
  }) => {
    await setupMap(page, map);
    await page.emulateMedia({ colorScheme: 'dark' });
    const canvas = await openMap(page);
    await setScheme(page, 'dark');
    const before = await debugInfo(canvas);
    await page.getByTestId('map-search').fill('TM-416');
    await expect(page.getByTestId('map-match-count')).toHaveText('1');
    await page.getByTestId('map-search').press('Enter');
    await page.waitForTimeout(1100);
    const after = await debugInfo(canvas);
    expect(after.k).toBeGreaterThan(before.k * 2);
    await shot(page, 'map-search-dark');
  });

  test('with the graph feature off the Map entry is gone and the page redirects', async ({
    page,
  }) => {
    await setupMap(page, map, {}, { graph: false });
    await page.goto('/demo/list');
    await expect(page.getByRole('link', { name: 'Archive' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Map' })).toHaveCount(0);
    await page.goto('/demo/map');
    await expect(page).not.toHaveURL(/\/map$/);
    await expect(page.getByTestId('workspace-map')).toHaveCount(0);
  });

  test('with the graph feature on the Map entry opens the page', async ({
    page,
  }) => {
    await setupMap(page, map);
    await page.goto('/demo/list');
    await page.getByRole('link', { name: 'Map' }).click();
    await expect(page).toHaveURL(/\/demo\/map$/);
  });

  test('search results are keyboard operable', async ({ page }) => {
    await setupMap(page, map);
    await openMap(page);
    const search = page.getByTestId('map-search');
    await search.fill('overview');
    const list = page.getByTestId('map-results');
    await expect(list.getByRole('option').first()).toBeVisible();
    await search.press('ArrowDown');
    await search.press('ArrowDown');
    await expect(list.getByRole('option').nth(1)).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await search.press('Escape');
    await expect(list).toHaveCount(0);
    await search.press('ArrowDown');
    await search.press('Enter');
    await expect(page.getByTestId('graph-overlay')).toBeVisible();
    await expect(page.getByTestId('map-canvas')).toHaveAttribute(
      'aria-label',
      /clusters: .*Bottleneck: TM-416 blocks 4/,
    );
  });

  test('the timeline hides newer items and the range buttons refetch', async ({
    page,
  }) => {
    const requests = await setupMap(page, map);
    const canvas = await openMap(page);
    const full = (await debugInfo(canvas)).visible;
    await page.getByTestId('map-scrubber').evaluate((el) => {
      el.value = '0';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await page.waitForTimeout(900);
    const early = (await debugInfo(canvas)).visible;
    expect(early).toBeLessThan(full);
    await page.getByTestId('map-play').click();
    await page.waitForTimeout(1500);
    expect((await debugInfo(canvas)).visible).toBeGreaterThan(early);
    await page.getByTestId('range-all').click();
    await expect.poll(() => requests.length).toBe(2);
    expect(requests[1].from).toBeUndefined();
    await page.getByTestId('range-7').click();
    await expect.poll(() => requests.length).toBe(3);
    expect(requests[2].from).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  test('items with no links can be hidden and focused', async ({ page }) => {
    await setupMap(page, map);
    const canvas = await openMap(page);
    expect((await debugInfo(canvas)).rings).toBeGreaterThan(0);
    const toggle = page.getByTestId('toggle-orphans');
    await expect(toggle).toHaveText(/Hide items with no links/);
    await toggle.click();
    await page.waitForTimeout(700);
    expect((await debugInfo(canvas)).rings).toBe(0);
    await page.getByTestId('insight-orphans').click();
    await expect(toggle).toHaveText(/Hide items with no links/);
    await page.waitForTimeout(700);
    expect((await debugInfo(canvas)).rings).toBeGreaterThan(0);
  });

  test('an insight focuses its nodes', async ({ page }) => {
    await setupMap(page, map);
    const canvas = await openMap(page);
    const before = await debugInfo(canvas);
    await page.getByTestId('insight-bottleneck').click();
    await page.waitForTimeout(1100);
    expect((await debugInfo(canvas)).k).toBeGreaterThan(before.k);
  });

  test('empty and truncated states', async ({ page }) => {
    const empty = { ...map, nodes: [], edges: [], total: 0 };
    await setupMap(page, empty);
    await page.goto('/demo/map');
    await expect(page.getByTestId('map-empty')).toBeVisible();
    await expect(page.getByText('Nothing to map in this period')).toBeVisible();
    await shot(page, 'map-empty-dark');
  });

  test('shows a notice when the server truncated the map', async ({ page }) => {
    await setupMap(page, map, { truncated: true, total: 9000 });
    await openMap(page);
    await expect(page.getByTestId('map-truncated')).toContainText(
      'of 9000 — narrow the time range',
    );
  });

  test('phone layout: controls wrap and the panel is a bottom sheet', async ({
    page,
  }) => {
    await setupMap(page, map);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    await openMap(page);
    await setScheme(page, 'dark');
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    await expect(page.getByTestId('map-sheet')).toBeVisible();
    await expect(page.getByTestId('map-insights')).toHaveCount(0);
    await shot(page, 'map-dark-390');
    await page.getByTestId('map-sheet-toggle').click();
    await expect(page.getByTestId('map-insights')).toBeVisible();
    await shot(page, 'map-dark-390-sheet');
  });
});
