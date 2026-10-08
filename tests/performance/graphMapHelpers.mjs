import { filterByRange } from './graphMapFixture.mjs';
import { mockApp } from './mockApp.mjs';

export const setupMap = async (page, map, patch = {}) => {
  await mockApp(page);
  const requests = [];
  await page.route('**/api/graph/map**', (route) => {
    const q = new URL(route.request().url()).searchParams;
    requests.push(Object.fromEntries(q.entries()));
    const data = map ? filterByRange(map, q.get('from'), q.get('to')) : map;
    return route.fulfill({ json: { data: { ...data, ...patch } } });
  });
  await page.route('**/api/graph/related**', (route) => {
    const entity = new URL(route.request().url()).searchParams.get('entity');
    return route.fulfill({
      json: {
        data: {
          center: entity,
          nodes: [
            {
              id: entity,
              type: entity.startsWith('page') ? 'page' : 'task',
              ref_id: 1,
              key: 'TM-1',
              title: 'Opened from the map',
              status: null,
              category_id: null,
              category: null,
              hop: 0,
              weight: 1,
              rel: null,
              meta: { slug: 'x' },
            },
          ],
          edges: [],
          truncated: false,
          caps_hit: {},
        },
      },
    });
  });
  await page.route('**/api/**/feature-toggles', (route) =>
    route.fulfill({
      json: {
        data: route.request().url().includes('/user/')
          ? {
              default_landing_page: {
                key: 'default_landing_page',
                name: 'Default Landing Page',
                group: 'general',
                type: 'select',
                options: ['list'],
                value: 'list',
              },
            }
          : {},
      },
    }),
  );
  return requests;
};

export const openMap = async (page) => {
  await page.goto('/demo/map');
  const canvas = page.getByTestId('map-canvas');
  await canvas.waitFor();
  await page.waitForSelector('[data-testid="map-canvas"][data-settled="1"]', {
    timeout: 30000,
  });
  await page.waitForTimeout(500);
  return canvas;
};

export const debugInfo = async (canvas) =>
  JSON.parse(await canvas.getAttribute('data-debug'));

export const setScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.evaluate(
    (dark) => document.documentElement.classList.toggle('dark', dark),
    scheme === 'dark',
  );
  await page.waitForTimeout(400);
};
