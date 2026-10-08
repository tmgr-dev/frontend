import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { mockApp } from './mockApp.mjs';

const shotDir = process.env.GRAPH_SHOTS;
if (shotDir) mkdirSync(shotDir, { recursive: true });

const setScheme = async (page, scheme) => {
  await page.emulateMedia({ colorScheme: scheme });
  await page.evaluate(
    (dark) => document.documentElement.classList.toggle('dark', dark),
    scheme === 'dark',
  );
  await page.waitForTimeout(250);
};

const shot = async (target, name) => {
  if (!shotDir) return;
  await target.screenshot({ path: `${shotDir}/${name}.png` });
};

const n = (id, type, hop, extra = {}) => ({
  id,
  type,
  ref_id: Number(String(id).split(':')[1]) || null,
  key: null,
  title: id,
  status: null,
  category_id: null,
  category: null,
  hop,
  weight: hop === 1 ? 2 : 1,
  rel: null,
  meta: {},
  ...extra,
});
const task = (num, title, hop, extra = {}) =>
  n(`task:${num}`, 'task', hop, {
    key: `TM-${398 + num}`,
    title,
    status: { name: 'In progress', type: 'active' },
    category_id: 1,
    category: 'Mobile',
    ...extra,
  });
const e = (type, label, from, to, weight, why) => ({
  id: `${type}:${from}:${to}`,
  from,
  to,
  type,
  label,
  weight,
  why,
});

const nodes = [
  task(1, 'Critical alarms: silent push → call escalation', 0, {
    weight: 5,
    status: { name: 'In progress', type: 'active' },
  }),
  task(2, 'AlarmKit on iPhone', 1, {
    weight: 3,
    status: { name: 'Review', type: 'active' },
  }),
  task(3, 'Alarm phone · mobile', 1, { weight: 3 }),
  task(4, 'tmgr-notify → npm', 1),
  n('page:10', 'page', 1, {
    ref_id: 10,
    title: 'Alarm contract',
    meta: { slug: 'alarm-contract', page_type: 'meeting' },
  }),
  n('page:11', 'page', 1, {
    ref_id: 11,
    title: 'Twilio setup',
    meta: { slug: 'twilio-setup', page_type: 'plain' },
  }),
  n('user:7', 'user', 1, { ref_id: 7, title: 'Alex Rivera', weight: 1 }),
  n('persona:8', 'persona', 1, { ref_id: 8, title: 'Scout', weight: 1 }),
  n('run:1:claude-code', 'agent_run', 1, {
    ref_id: 98,
    title: 'claude-code',
    status: { name: 'succeeded', type: 'succeeded' },
    weight: 1,
  }),
  task(5, 'Notification channels', 2),
  task(6, 'Telegram bridge', 2),
  task(7, 'Release checklist', 2),
  task(8, 'Push token rotation', 2),
  task(9, 'Fallback SMS', 2),
  n('page:12', 'page', 2, {
    ref_id: 12,
    title: 'Agents setup',
    meta: { slug: 'agents-setup', page_type: 'plain' },
  }),
  n('page:13', 'page', 2, {
    ref_id: 13,
    title: 'Escalation policy',
    meta: { slug: 'escalation-policy', page_type: 'plain' },
  }),
  n('persona:14', 'persona', 2, { ref_id: 14, title: 'Scout', weight: 1 }),
  n('run:3:codex', 'agent_run', 2, {
    ref_id: 99,
    title: 'codex review',
    status: { name: 'succeeded', type: 'succeeded' },
    weight: 1,
  }),
  n('user:15', 'user', 2, { ref_id: 15, title: 'Priya N.', weight: 1 }),
  task(10, 'Call provider evaluation', 2),
  task(11, 'Quiet hours', 2),
  n('page:16', 'page', 2, {
    ref_id: 16,
    title: 'Runbook',
    meta: { slug: 'runbook', page_type: 'plain' },
  }),
  task(12, 'Delivery receipts', 2),
  task(13, 'Alarm sounds', 2),
  n('run:5:claude-code', 'agent_run', 2, {
    ref_id: 100,
    title: 'claude-code',
    status: { name: 'failed', type: 'failed' },
    weight: 1,
  }),
];
const edges = [
  e(
    'depends_on',
    'depends on',
    'task:2',
    'task:1',
    3,
    'TM-400 depends on TM-399 (task relation)',
  ),
  e(
    'depends_on',
    'depends on',
    'task:3',
    'task:1',
    3,
    'TM-401 depends on TM-399 (task relation)',
  ),
  e(
    'relates_to',
    'relates to',
    'task:1',
    'task:4',
    2,
    'TM-399 relates to TM-402 (task relation)',
  ),
  e(
    'mentioned_in',
    'mentioned in',
    'task:1',
    'page:10',
    2,
    'TM-399 is mentioned in Alarm contract',
  ),
  e(
    'linked_page',
    'linked page',
    'page:11',
    'task:1',
    2,
    'Twilio setup links TM-399',
  ),
  e(
    'assignee',
    'assignee',
    'task:1',
    'user:7',
    1,
    'Alex Rivera is assigned to TM-399',
  ),
  e(
    'persona_assignee',
    'assignee',
    'task:1',
    'persona:8',
    1,
    'Scout is assigned to TM-399',
  ),
  e(
    'worked_on',
    'worked on',
    'run:1:claude-code',
    'task:1',
    1,
    'claude-code worked on TM-399 (3 runs)',
  ),
  e(
    'mentioned_in',
    'mentioned in',
    'task:2',
    'page:10',
    2,
    'TM-400 is mentioned in Alarm contract',
  ),
  e(
    'relates_to',
    'relates to',
    'task:2',
    'task:3',
    2,
    'TM-400 relates to TM-401 (task relation)',
  ),
  e(
    'depends_on',
    'depends on',
    'task:2',
    'task:5',
    3,
    'TM-400 depends on TM-403',
  ),
  e('blocks', 'blocks', 'task:2', 'task:6', 3, 'TM-400 blocks TM-404'),
  e(
    'relates_to',
    'relates to',
    'task:3',
    'task:7',
    2,
    'TM-401 relates to TM-405',
  ),
  e(
    'depends_on',
    'depends on',
    'task:3',
    'task:8',
    3,
    'TM-401 depends on TM-406',
  ),
  e(
    'relates_to',
    'relates to',
    'task:3',
    'task:9',
    2,
    'TM-401 relates to TM-407',
  ),
  e(
    'relates_to',
    'relates to',
    'task:4',
    'task:10',
    2,
    'TM-402 relates to TM-408',
  ),
  e(
    'linked_page',
    'linked page',
    'page:12',
    'task:4',
    2,
    'Agents setup links TM-402',
  ),
  e(
    'links_to',
    'links to',
    'page:11',
    'page:13',
    2,
    'Twilio setup links to Escalation policy',
  ),
  e(
    'links_to',
    'links to',
    'page:10',
    'page:16',
    2,
    'Alarm contract links to Runbook',
  ),
  e(
    'mentions',
    'mentions',
    'page:10',
    'persona:14',
    1,
    'Alarm contract mentions Scout',
  ),
  e(
    'worked_on',
    'worked on',
    'run:3:codex',
    'task:5',
    1,
    'codex worked on TM-403',
  ),
  e(
    'assignee',
    'assignee',
    'task:6',
    'user:15',
    1,
    'Priya N. is assigned to TM-404',
  ),
  e(
    'relates_to',
    'relates to',
    'task:7',
    'task:11',
    2,
    'TM-405 relates to TM-409',
  ),
  e(
    'relates_to',
    'relates to',
    'task:8',
    'task:12',
    2,
    'TM-406 relates to TM-410',
  ),
  e(
    'relates_to',
    'relates to',
    'task:9',
    'task:13',
    2,
    'TM-407 relates to TM-411',
  ),
  e(
    'worked_on',
    'worked on',
    'run:5:claude-code',
    'task:9',
    1,
    'claude-code worked on TM-407',
  ),
];

const graphFor = (entity, depth, center = 'task:1') => {
  const maxHop = depth === 1 ? 1 : 2;
  const keep = new Set(nodes.filter((x) => x.hop <= maxHop).map((x) => x.id));
  const list = nodes
    .filter((x) => keep.has(x.id))
    .map((x) => (x.id === center ? x : x.hop === 0 ? { ...x, hop: 1 } : x));
  return {
    center,
    nodes: list,
    edges: edges.filter((x) => keep.has(x.from) && keep.has(x.to)),
    truncated: false,
    caps_hit: {},
  };
};

const pageCenter = {
  id: 'page:1',
  type: 'page',
  ref_id: 1,
  key: null,
  title: 'Ann Lee',
  status: null,
  category_id: null,
  category: null,
  hop: 0,
  weight: 4,
  rel: null,
  meta: { slug: 'ann-lee', page_type: 'person' },
};

const setup = async (page, { graph = true } = {}) => {
  await mockApp(page);
  const requests = [];
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
          : {
              board: toggle('board'),
              pages: toggle('pages'),
              ...(graph ? { graph: toggle('graph') } : {}),
            },
      },
    });
  });
  await page.route('**/api/graph/related**', (route) => {
    const url = new URL(route.request().url());
    const entity = url.searchParams.get('entity');
    const depth = Number(url.searchParams.get('depth') || 2);
    requests.push({ entity, depth, include: url.searchParams.get('include') });
    if (entity === 'page:ann-lee') {
      const base = graphFor('x', depth);
      return route.fulfill({
        json: {
          data: {
            center: 'page:1',
            nodes: [pageCenter, ...base.nodes.filter((x) => x.hop === 1)],
            edges: base.edges
              .filter((x) => x.from === 'task:1' || x.to === 'task:1')
              .map((x) => ({
                ...x,
                from: x.from === 'task:1' ? 'page:1' : x.from,
                to: x.to === 'task:1' ? 'page:1' : x.to,
                id: `p${x.id}`,
              })),
            truncated: false,
            caps_hit: {},
          },
        },
      });
    }
    const center = entity && entity.startsWith('task:') ? entity : 'task:1';
    return route.fulfill({
      json: { data: graphFor(entity, depth, center) },
    });
  });
  await page.route('**/api/workspaces/1/categories*', (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.route('**/api/pages**', (route) => {
    const path = new URL(route.request().url()).pathname
      .replace('/api/pages', '')
      .replace(/^\//, '');
    const summary = {
      id: 1,
      title: 'Ann Lee',
      slug: 'ann-lee',
      type: 'person',
      parent_id: null,
      position: 1,
      pinned: false,
      updated_at: '2026-10-01T10:00:00Z',
    };
    const json = (data) => route.fulfill({ json: { data } });
    if (path === 'tree' || path === '') return json([summary]);
    if (path === 'ann-lee')
      return json({
        ...summary,
        workspace_id: 1,
        body: '## Summary\n\nPromised the report.\n',
        properties: { network: 'operational', aliases: [] },
        version: 1,
        author: { kind: 'user', id: 1, name: 'Test User' },
        updated_by: { kind: 'user', id: 1, name: 'Test User' },
        created_at: '2026-09-30T10:00:00Z',
        backlinks: [],
        sections: [],
        files: [],
        following: false,
      });
    return json([]);
  });
  return requests;
};

const openTaskGraph = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  const form = page.locator('.new-form-container');
  await expect(form.getByPlaceholder('Task name')).toHaveValue('Original task');
  const section = form.getByTestId('entity-graph');
  await section.scrollIntoViewIfNeeded();
  await expect(section.getByTestId('graph-legend')).toBeVisible();
  await page.waitForTimeout(3200);
  return { form, section };
};

const pointerNodes = async (canvas) => {
  const box = await canvas.boundingBox();
  const found = await canvas.evaluate((el) => {
    const rect = el.getBoundingClientRect();
    const hits = [];
    for (let y = 30; y < rect.height - 30; y += 8) {
      for (let x = 30; x < rect.width - 30; x += 8) {
        el.dispatchEvent(
          new PointerEvent('pointermove', {
            clientX: rect.left + x,
            clientY: rect.top + y,
            pointerId: 1,
            bubbles: true,
          }),
        );
        if (el.style.cursor === 'pointer') hits.push({ x, y });
      }
    }
    el.dispatchEvent(new PointerEvent('pointerleave'));
    return hits;
  });
  return found.map((p) => ({ x: box.x + p.x, y: box.y + p.y }));
};

const selectNode = async (page, overlay, text) => {
  const canvas = overlay.locator('canvas');
  const points = await pointerNodes(canvas);
  for (const p of points) {
    await page.mouse.click(p.x, p.y);
    const label = await overlay.getByTestId('graph-selected').innerText();
    if (label.includes(text)) return;
  }
  throw new Error(`node ${text} not found`);
};

test.describe('Graph neighbourhood view', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
  });

  test('the task modal has no graph section while the graph feature is off', async ({
    page,
  }) => {
    await setup(page, { graph: false });
    await page.goto('/demo/list');
    await page.locator('[data-task-id="1"]').first().click();
    const form = page.locator('.new-form-container');
    await expect(form.getByPlaceholder('Task name')).toHaveValue(
      'Original task',
    );
    await page.waitForTimeout(800);
    await expect(form.getByTestId('entity-graph')).toHaveCount(0);
  });

  test('task modal renders the mini graph only after it scrolls into view', async ({
    page,
  }) => {
    const requests = await setup(page);
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('/demo/list');
    await page.locator('[data-task-id="1"]').first().click();
    const form = page.locator('.new-form-container');
    await expect(form.getByPlaceholder('Task name')).toHaveValue(
      'Original task',
    );
    await expect(form.getByTestId('entity-graph')).toHaveCount(1);
    expect(requests.length).toBeLessThanOrEqual(1);
    const { section } = await openTaskGraph(page);
    expect(requests.some((r) => r.entity === 'task:1' && r.depth === 1)).toBe(
      true,
    );
    await expect(section.getByRole('button', { name: 'Expand' })).toBeVisible();
    await setScheme(page, 'dark');
    await shot(section, 'task-mini-dark');
    await setScheme(page, 'light');
    await shot(section, 'task-mini-light');
  });

  test('overlay opens, selects a node, re-centres, filters and closes with Esc', async ({
    page,
  }) => {
    const requests = await setup(page);
    await page.emulateMedia({ colorScheme: 'dark' });
    const { form, section } = await openTaskGraph(page);
    await section.getByRole('button', { name: 'Expand' }).click();
    const overlay = page.getByTestId('graph-overlay');
    await expect(overlay).toBeVisible();
    await expect(overlay.getByText('Mobile · TM-399')).toBeVisible();
    await expect(
      overlay.getByRole('heading', {
        name: 'Critical alarms: silent push → call escalation',
      }),
    ).toBeVisible();
    await page.waitForTimeout(3500);
    await setScheme(page, 'dark');
    await shot(overlay, 'overlay-center-dark');

    await selectNode(page, overlay, 'TM-400');
    await expect(overlay.getByTestId('graph-why')).toContainText(
      'TM-400 depends on TM-399',
    );
    await expect(overlay.getByTestId('graph-open')).toHaveText('Open task');
    await page.mouse.move(2, 2);
    await page.waitForTimeout(900);
    await shot(overlay, 'overlay-selected-dark');
    await setScheme(page, 'light');
    await page.waitForTimeout(500);
    await shot(overlay, 'overlay-selected-light');
    await setScheme(page, 'dark');

    await overlay.getByTestId('graph-center-here').click();
    await expect
      .poll(() => requests.some((r) => r.entity === 'task:2'))
      .toBe(true);

    await overlay.getByRole('button', { name: '1 step' }).click();
    await overlay.getByLabel('Pages').uncheck();
    await expect
      .poll(() =>
        requests.some(
          (r) =>
            r.depth === 1 &&
            r.include &&
            !r.include.includes('pages') &&
            r.include.includes('tasks'),
        ),
      )
      .toBe(true);

    await page.keyboard.press('Escape');
    await expect(overlay).toHaveCount(0);
    await expect(form.getByPlaceholder('Task name')).toBeVisible();
  });

  test('page side panel shows the mini graph', async ({ page }) => {
    await setup(page);
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/demo/pages/ann-lee');
    const panel = page.getByTestId('page-side-panel');
    const section = panel.getByTestId('entity-graph');
    await section.scrollIntoViewIfNeeded();
    await expect(section.locator('canvas')).toBeVisible();
    await page.waitForTimeout(3000);
    await setScheme(page, 'light');
    await shot(page, 'page-panel-light');
    await setScheme(page, 'dark');
    await page.waitForTimeout(400);
    await shot(page, 'page-panel-dark');
    await section.getByRole('button', { name: 'Expand' }).click();
    await expect(page.getByTestId('graph-overlay')).toBeVisible();
    await expect(page.getByTestId('graph-open')).toHaveText('Open page');
  });

  test('overlay turns the panel into a bottom sheet at 390px', async ({
    page,
  }) => {
    await setup(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    const { section } = await openTaskGraph(page);
    await section.getByRole('button', { name: 'Expand' }).click();
    const overlay = page.getByTestId('graph-overlay');
    await expect(overlay).toBeVisible();
    await page.waitForTimeout(3500);
    const canvasBox = await overlay.locator('canvas').boundingBox();
    const panelBox = await overlay.getByTestId('graph-panel').boundingBox();
    expect(panelBox.y).toBeGreaterThanOrEqual(
      canvasBox.y + canvasBox.height - 2,
    );
    expect(panelBox.width).toBeGreaterThan(340);
    await setScheme(page, 'dark');
    await shot(page, 'overlay-390-dark');
    await setScheme(page, 'light');
    await page.waitForTimeout(400);
    await shot(page, 'overlay-390-light');
  });
});
