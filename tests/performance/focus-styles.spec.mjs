import { writeFileSync } from 'node:fs';

import { expect, test } from '@playwright/test';

import { buildGraphMap } from './graphMapFixture.mjs';
import { openMap, setupMap } from './graphMapHelpers.mjs';
import { mockApp } from './mockApp.mjs';

// FOCUS_SHOTS_DIR=<abs dir> also saves light/dark screenshots of mouse vs keyboard focus.
const SHOTS = process.env.FOCUS_SHOTS_DIR;
const MAX_STOPS = 70;

test.use({ actionTimeout: 10_000 });

const me = { id: 1, name: 'Test User', email: 'test@example.test' };
const ann = { id: 2, name: 'Ann Lee', email: 'ann@example.test' };

const memberStats = (user) => ({
  ...user,
  has_avatar: false,
  role: 'member',
  is_online: true,
  last_activity_at: '2026-10-12T10:00:00Z',
  last_task: null,
  current_task: null,
  tracked_seconds: 3600,
  done_count: 2,
  active_tasks: 1,
  completed_tasks: 2,
  comments_count: 1,
  activity_count: 3,
  daily_activity: [],
  streak: 1,
  member_since: '2026-01-01T10:00:00Z',
  assigned_count: 1,
  created_count: 1,
  commented_tasks_count: 1,
  active_days: 1,
  longest_streak: 2,
  team: { tracked_seconds: 3600, done_count: 2, total_members: 2 },
  window: '7d',
});

const statistics = (n) => ({
  total_tasks: 12 * n,
  active_tasks: 4 * n,
  completed_today: n,
  completed_week: 3 * n,
  completed_month: 8 * n,
  time_today: 3600 * n,
  time_week: 7200 * n,
  time_month: 36000 * n,
  time_total: 72000 * n,
  team_members: n ? 2 : 1,
  overdue_tasks: n,
  daily_routine_completion_rate: 40 * n,
});

const heatmap = () => {
  const contributions = [];
  const start = new Date();
  start.setDate(start.getDate() - 364);
  for (let i = 0; i < 365; i++) {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    contributions.push({
      date: d.toISOString().slice(0, 10),
      count: i % 5,
      level: i % 5,
    });
  }
  return {
    contributions,
    total_contributions: 700,
    streak: { current: 3, longest: 9 },
    year: new Date().getFullYear(),
    weeks: 53,
  };
};

const activity = {
  id: 1,
  type: 'task_created',
  title: 'created a task',
  subject_name: 'Original task',
  description: 'Test User created Original task',
  user: me,
  workspace_id: 1,
  subject_type: 'task',
  subject_id: 1,
  metadata: {},
  created_at: '2026-10-12T10:00:00Z',
  timestamp_human: '1 day ago',
};

const recentTask = {
  id: 1,
  title: 'Original task',
  status: { id: 1, name: 'Backlog', type: 'default', color: '#888888' },
  user: me,
  assignees: [me],
  updated_at: '2026-10-12T10:00:00Z',
  updated_at_human: '1 day ago',
  timer_running: false,
  is_overdue: false,
};

const routine = () => {
  const d = new Date();
  return {
    task_id: 31,
    title: 'Stretch',
    description: null,
    routine_category: { id: 'health', name: 'Health', color: '#22c55e' },
    date: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(
      2,
      '0',
    )}-${String(d.getDate()).padStart(2, '0')}`,
    time: '08:15',
    duration_min: 20,
    reminder_min: 15,
    completed: false,
    status: 'pending',
    instance_id: null,
    virtual: true,
    frequency: 'DAILY',
    scheduled_for: null,
  };
};

const installProbe = () => {
  const alpha = (c) => {
    const m = c.match(/rgba?\(([^)]+)\)/);
    if (!m) return c === 'transparent' ? 0 : 1;
    const parts = m[1].split(/[ ,/]+/).filter(Boolean);
    return parts.length > 3 ? Number(parts[3]) : 1;
  };
  const chain = (el) => {
    const out = [];
    for (let n = el; n && n !== document.documentElement; n = n.parentElement)
      out.push(n);
    return out;
  };
  const settle = async (els) => {
    els.forEach((e) => getComputedStyle(e).boxShadow);
    await new Promise((r) =>
      requestAnimationFrame(() => requestAnimationFrame(r)),
    );
    const running = els
      .flatMap((e) => e.getAnimations())
      .filter((a) => a instanceof CSSTransition);
    await Promise.race([
      Promise.all(running.map((a) => a.finished.catch(() => {}))),
      new Promise((r) => setTimeout(r, 400)),
    ]);
  };
  const look = (e) => {
    const cs = getComputedStyle(e);
    return {
      outline: `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor} ${cs.outlineOffset}`,
      outlineDrawn:
        cs.outlineStyle !== 'none' &&
        parseFloat(cs.outlineWidth) > 0 &&
        alpha(cs.outlineColor) > 0,
      boxShadow: cs.boxShadow,
      border: cs.borderColor,
    };
  };
  let seq = 0;
  const name = (e) => {
    const role = e.getAttribute('role');
    const label = (
      e.getAttribute('aria-label') ||
      e.getAttribute('placeholder') ||
      e.textContent ||
      ''
    )
      .replace(/\s+/g, ' ')
      .trim()
      .slice(0, 40);
    return `${e.tagName.toLowerCase()}${role ? `[role=${role}]` : ''}${
      e.getAttribute('tabindex') !== null
        ? `[tabindex=${e.getAttribute('tabindex')}]`
        : ''
    } "${label}"`;
  };
  const TAGS = ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'SUMMARY'];
  const ROLES = [
    'button',
    'link',
    'tab',
    'switch',
    'checkbox',
    'radio',
    'menuitem',
    'menuitemcheckbox',
    'menuitemradio',
    'option',
    'combobox',
    'textbox',
    'searchbox',
    'slider',
    'spinbutton',
    'treeitem',
  ];
  const NON_TEXT = [
    'checkbox',
    'radio',
    'button',
    'submit',
    'reset',
    'range',
    'color',
    'file',
    'image',
  ];
  const kind = (e) => {
    if (e.isContentEditable) return 'editor';
    // D-009: the task title is an inline heading editor, exempt from D-003.
    if (e.matches('[data-tour="task-form.title"]')) return 'editor';
    if (
      e.tagName === 'TEXTAREA' ||
      (e.tagName === 'INPUT' && !NON_TEXT.includes(e.type))
    )
      return 'text';
    if (
      /^(menuitem|menuitemcheckbox|menuitemradio|option)$/.test(
        e.getAttribute('role') || '',
      )
    )
      return 'menu';
    return 'control';
  };
  window.__qaFocus = {
    async probe(target) {
      const el = target || document.activeElement;
      if (!el || el === document.body || el === document.documentElement)
        return null;
      if (!el.dataset.qaId) el.dataset.qaId = String(++seq);
      const els = chain(el);
      await settle(els);
      const cs = getComputedStyle(el);
      const focused = els.map(look);
      const res = {
        qaId: el.dataset.qaId,
        el: name(el),
        tag: el.tagName.toLowerCase(),
        role: el.getAttribute('role'),
        kind: kind(el),
        interactive:
          TAGS.includes(el.tagName) ||
          ROLES.includes(el.getAttribute('role')) ||
          el.isContentEditable,
        isActive: el === document.activeElement,
        focusVisible: el.matches(':focus-visible'),
        noBand: el.matches(
          "[type='checkbox'], [type='radio'], [role='switch']",
        ),
        outlineStyle: cs.outlineStyle,
        outlineWidth: parseFloat(cs.outlineWidth),
        outlineColor: cs.outlineColor,
        outlineOffset: parseFloat(cs.outlineOffset),
        outlineDrawn: focused[0].outlineDrawn,
        border: cs.borderColor,
        background: cs.backgroundColor,
        shot: el.closest('[data-qa-shot]')?.dataset.qaShot ?? null,
        decorations: [],
      };
      if (!res.isActive) return res;
      el.blur();
      await settle(els);
      const blurred = els.map(look);
      res.blurredBorder = getComputedStyle(el).borderColor;
      // D-003: a borderless input inside a bordered wrapper shows focus on the wrapper.
      const changed = els
        .slice(0, 4)
        .findIndex((e, i) => focused[i].border !== blurred[i].border);
      res.focusBorder =
        changed < 0 ? null : `${name(els[changed])} ${focused[changed].border}`;
      res.focusBorderColor = changed < 0 ? null : focused[changed].border;
      // D-010: the element's own focus box-shadow (the inner band) is checked
      // per kind by the callers; ancestors must not change.
      res.ownShadow =
        focused[0].boxShadow !== blurred[0].boxShadow
          ? focused[0].boxShadow
          : null;
      els.forEach((e, i) => {
        if (i && focused[i].boxShadow !== blurred[i].boxShadow)
          res.decorations.push(
            `ancestor ${name(e)} box-shadow on focus: ${focused[i].boxShadow}`,
          );
        if (i && focused[i].outline !== blurred[i].outline)
          res.decorations.push(
            `ancestor ${name(e)} outline on focus: ${focused[i].outline}`,
          );
      });
      el.focus({ preventScroll: true });
      return res;
    },
    async reference() {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = 'qa focus reference';
      (document.getElementById('app') || document.body).prepend(b);
      b.focus();
      await settle([b]);
      const cs = getComputedStyle(b);
      const d = document.createElement('div');
      d.style.border = '1px solid var(--focus-border)';
      b.after(d);
      const res = {
        focusVisible: b.matches(':focus-visible'),
        tuple: `${cs.outlineColor} ${cs.outlineWidth} ${cs.outlineOffset}`,
        outlineStyle: cs.outlineStyle,
        boxShadow: cs.boxShadow,
        focusBorder: getComputedStyle(d).borderColor,
      };
      b.remove();
      d.remove();
      return res;
    },
  };
};

const tupleOf = (s) =>
  `${s.outlineColor} ${s.outlineWidth}px ${s.outlineOffset}px`;

const firstLayer = (shadow) =>
  shadow && shadow !== 'none' ? shadow.split(/,(?![^(]*\))/)[0].trim() : null;
const layerColor = (layer) =>
  layer?.match(/^(rgba?\([^)]*\)|color\([^)]*\))/)?.[1] ?? null;

async function setup(
  page,
  scheme,
  { empty = false, failStatistics = false } = {},
) {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.addInitScript((s) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('colorScheme', s);
  }, scheme);
  await page.addInitScript(installProbe);
  await mockApp(page, { taskCount: empty ? 0 : 3, multiStatus: true });
  await page.route(
    (u) =>
      u.pathname.startsWith('/api/') &&
      u.pathname.endsWith('feature-toggles') &&
      !u.pathname.includes('/user'),
    (r) =>
      r.fulfill({
        json: {
          data: {
            board: { enabled: true },
            dashboard: { enabled: true },
            daily_routines: { enabled: true },
          },
        },
      }),
  );
  await page.route('**/api/workspaces/1/members*', (r) =>
    r.fulfill({ json: { data: empty ? [me] : [me, ann] } }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      const stats = p.match(/dashboard\/members\/(\d+)$/);
      if (stats)
        return r.fulfill({
          json: {
            data: memberStats(
              [me, ann].find((m) => m.id === Number(stats[1])) || me,
            ),
          },
        });
      if (/dashboard\/members\/\d+\/tasks$/.test(p))
        return r.fulfill({
          json: { data: [], total: 0, page: 1, per_page: 20 },
        });
      if (p.endsWith('activities'))
        return r.fulfill({
          json: {
            data: empty ? [] : [activity],
            meta: {
              current_page: 1,
              per_page: 20,
              total: empty ? 0 : 1,
              last_page: 1,
            },
            links: {},
          },
        });
      if (p.endsWith('heatmap'))
        return r.fulfill({ json: { data: heatmap() } });
      if (p.endsWith('team-activity'))
        return r.fulfill({
          json: {
            data: {
              members: empty ? [] : [memberStats(me), memberStats(ann)],
              total_members: empty ? 0 : 2,
              online_members: empty ? 0 : 2,
              active_today: empty ? 0 : 2,
              active_timers: 0,
              tracked_seconds: empty ? 0 : 3600,
              done_count: empty ? 0 : 2,
              window: 'today',
            },
          },
        });
      if (p.endsWith('recent-tasks'))
        return r.fulfill({ json: { data: empty ? [] : [recentTask] } });
      if (p.endsWith('statistics'))
        return failStatistics
          ? r.fulfill({ status: 500, json: { message: 'Statistics failed' } })
          : r.fulfill({ json: { data: statistics(empty ? 0 : 1) } });
      return r.fulfill({ json: { data: {} } });
    },
  );
  await page.route('**/api/daily-routines/**', (r) => {
    const p = new URL(r.request().url()).pathname;
    if (/daily-routines\/expand\/stats/.test(p))
      return r.fulfill({ json: { data: {} } });
    if (/daily-routines\/expand/.test(p))
      return r.fulfill({ json: { data: empty ? [] : [routine()] } });
    return r.fulfill({ json: { data: [] } });
  });
}

async function open(page, url, ready, scheme) {
  await page.goto(url);
  await expect(ready(page)).toBeVisible({ timeout: 60_000 });
  if (scheme === 'dark')
    await expect(page.locator('html')).toHaveClass(/\bdark\b/);
  else await expect(page.locator('html')).not.toHaveClass(/\bdark\b/);
}

const shot = async (page, file) => {
  if (!SHOTS) return;
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${SHOTS}/${file}.png` });
};

const dump = (file, data) => {
  if (SHOTS)
    writeFileSync(`${SHOTS}/${file}.json`, JSON.stringify(data, null, 2));
};

async function reference(page) {
  await page.keyboard.press('Tab');
  const ref = await page.evaluate(() => window.__qaFocus.reference());
  expect
    .soft(ref.focusVisible, 'reference button matches :focus-visible')
    .toBe(true);
  ref.band = firstLayer(ref.boxShadow);
  expect
    .soft(ref.band, 'reference ring has an inner 4px band (D-010)')
    .toMatch(/ 0px 0px 0px 4px inset$/);
  return ref;
}

async function walk(page, { max = MAX_STOPS, shots = [], tag = '' } = {}) {
  for (const [file, locate] of shots)
    await locate(page)
      .first()
      .evaluate((el, f) => (el.dataset.qaShot = f), file, { timeout: 5000 })
      .catch(() => {});
  const stops = [];
  const seen = new Set();
  const taken = new Set();
  let empty = 0;
  for (let i = 0; i < max; i++) {
    await page.keyboard.press('Tab');
    const s = await page.evaluate(() => window.__qaFocus.probe());
    if (!s) {
      if (++empty > 1) break;
      continue;
    }
    if (seen.has(s.qaId)) break;
    seen.add(s.qaId);
    stops.push(s);
    if (s.shot && !taken.has(s.shot)) {
      taken.add(s.shot);
      await shot(page, `${tag}-${s.shot}`);
    }
  }
  dump(`${tag}-stops`, stops);
  return stops;
}

function expectKeyboardStops(stops, ref, label, { min = 4 } = {}) {
  expect
    .soft(stops.length, `${label}: Tab reached some elements`)
    .toBeGreaterThanOrEqual(min);
  const fmt = (s, extra = '') => `${s.el} ${extra}`.trim();
  expect
    .soft(
      stops.filter((s) => !s.interactive).map((s) => fmt(s)),
      `${label}: non-interactive containers in the Tab order`,
    )
    .toEqual([]);
  const controls = stops.filter((s) => s.kind === 'control');
  expect
    .soft(
      controls
        .filter((s) => !s.outlineDrawn || !s.focusVisible)
        .map((s) =>
          fmt(
            s,
            `outline=${s.outlineStyle} ${tupleOf(s)} fv=${s.focusVisible}`,
          ),
        ),
      `${label}: Tab stops without a visible outline`,
    )
    .toEqual([]);
  if (controls.length)
    expect
      .soft(
        [...new Set(controls.map(tupleOf))],
        `${label}: one focus tuple (color width offset) on every Tab stop, equal to the global rule`,
      )
      .toEqual([ref.tuple]);
  expect
    .soft(
      controls
        .filter((s) => s.outlineOffset + s.outlineWidth > 0)
        .map((s) => fmt(s, tupleOf(s))),
      `${label}: outline not inset (D-002)`,
    )
    .toEqual([]);
  const banded = controls.filter((s) => !s.noBand);
  if (banded.length)
    expect
      .soft(
        [...new Set(banded.map((s) => firstLayer(s.ownShadow)))],
        `${label}: one inner band (first box-shadow layer) on every Tab stop (D-010)`,
      )
      .toEqual([ref.band]);
  // Checkbox, radio and switch get the outline only, so checked fills stay visible.
  expect
    .soft(
      controls
        .filter((s) => s.noBand && /4px inset$/.test(firstLayer(s.ownShadow)))
        .map((s) => fmt(s, s.ownShadow)),
      `${label}: checkbox/radio/switch draw no inner band`,
    )
    .toEqual([]);
  expect
    .soft(
      stops
        .filter((s) => s.kind !== 'control' && s.ownShadow)
        .map((s) => fmt(s, `${s.kind} box-shadow=${s.ownShadow}`)),
      `${label}: inputs/editors/menu items draw no focus box-shadow`,
    )
    .toEqual([]);
  expect
    .soft(
      stops.flatMap((s) => s.decorations.map((d) => `${s.el}: ${d}`)),
      `${label}: extra focus box-shadow/ring on an ancestor`,
    )
    .toEqual([]);
  expect
    .soft(
      stops
        .filter((s) => s.kind !== 'control' && s.outlineDrawn)
        .map((s) => fmt(s, `${s.kind} outline=${tupleOf(s)}`)),
      `${label}: inputs/editors/menu items must not draw an outline (D-003, D-005)`,
    )
    .toEqual([]);
  expect
    .soft(
      stops
        .filter((s) => s.kind === 'text' && !s.focusBorder)
        .map((s) => fmt(s, `border=${s.border}`)),
      `${label}: text inputs show focus by a border change on the input or its wrapper (D-003)`,
    )
    .toEqual([]);
  expect
    .soft(
      stops
        .filter(
          (s) =>
            s.kind === 'text' &&
            s.focusBorder &&
            s.focusBorderColor !== ref.focusBorder,
        )
        .map((s) => fmt(s, s.focusBorder)),
      `${label}: focus border is --focus-border (D-011)`,
    )
    .toEqual([]);
}

async function mouseProbe(
  page,
  locator,
  label,
  { stops = [], file, ref } = {},
) {
  const box = await locator
    .scrollIntoViewIfNeeded()
    .then(() => locator.boundingBox())
    .catch(() => null);
  expect.soft(box, `${label}: target is on screen`).not.toBeNull();
  if (!box) return null;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  const down = await page.evaluate(() => window.__qaFocus.probe());
  if (file) await shot(page, file);
  const self = await locator
    .evaluate(
      (el) =>
        el === document.activeElement ? null : window.__qaFocus.probe(el),
      null,
      { timeout: 2000 },
    )
    .catch(() => null);
  await page.mouse.up();
  await page.waitForTimeout(400);
  const after = await page.evaluate(() => window.__qaFocus.probe());
  const all = [down, self, after].filter(Boolean);
  expect
    .soft(
      all
        .filter((s) => s.outlineDrawn)
        .map((s) => `${s.el} outline=${tupleOf(s)}`),
      `${label}: mouse click draws no outline`,
    )
    .toEqual([]);
  expect
    .soft(
      all.flatMap((s) => s.decorations.map((d) => `${s.el}: ${d}`)),
      `${label}: mouse click draws no focus box-shadow/ring`,
    )
    .toEqual([]);
  expect
    .soft(
      all.filter((s) => s.ownShadow).map((s) => `${s.el} ${s.ownShadow}`),
      `${label}: mouse click draws no inner band`,
    )
    .toEqual([]);
  if (down?.kind === 'text') {
    expect
      .soft(
        down.focusBorder,
        `${label}: input or its wrapper border changes on click (D-003)`,
      )
      .not.toBeNull();
    if (ref)
      expect
        .soft(
          down.focusBorderColor,
          `${label}: click border is --focus-border (D-011)`,
        )
        .toBe(ref.focusBorder);
    const viaTab = stops.find((s) => s.qaId === down.qaId);
    if (viaTab)
      expect
        .soft(down.focusBorder, `${label}: same border on click and on Tab`)
        .toBe(viaTab.focusBorder);
  }
  return { down, after };
}

const textInput = (scope) =>
  scope
    .locator(
      'input[type="text"]:visible, input[type="search"]:visible, input:not([type]):visible',
    )
    .first();
const escape = (p) => p.keyboard.press('Escape');
const PAGES = [
  {
    name: 'dashboard',
    url: '/demo/dashboard',
    ready: (p) => p.locator('[aria-labelledby="statistics-heading"]'),
    shots: [
      [
        'tab-refresh',
        (p) => p.getByRole('button', { name: 'Refresh dashboard data' }),
      ],
      [
        'tab-sidebar-list',
        (p) => p.getByRole('link', { name: 'List', exact: true }),
      ],
    ],
    clicks: [
      [
        'workspace-statistics-heading',
        (p) => p.locator('#statistics-heading'),
        'click-workspace-statistics',
      ],
      [
        'refresh-button',
        (p) => p.getByRole('button', { name: 'Refresh dashboard data' }),
        'click-refresh',
      ],
      [
        'sidebar-dashboard-link',
        (p) => p.getByRole('link', { name: 'Dashboard', exact: true }).first(),
        'click-sidebar-link',
      ],
      [
        'total-tasks-card',
        (p) => p.getByText('Total Tasks', { exact: true }),
        'click-total-tasks',
      ],
    ],
    // D-006: the stat cards are not clickable, so nothing up to the section is focusable.
    check: async (p, label) =>
      expect
        .soft(
          await p.getByText('Total Tasks', { exact: true }).evaluate((el) => {
            for (let n = el; n && n.tagName !== 'SECTION'; n = n.parentElement)
              if (n.tabIndex >= 0 || n.getAttribute('role') === 'button')
                return n.outerHTML.slice(0, 120);
            return null;
          }),
          `${label}: Total Tasks card is not focusable (D-006)`,
        )
        .toBeNull(),
  },
  {
    name: 'board',
    url: '/demo/board',
    ready: (p) => p.locator('[data-task-id="1"]').first(),
    shots: [
      ['tab-filters', (p) => p.getByRole('button', { name: 'Filters' })],
      ['tab-card', (p) => p.locator('[data-task-id="1"]').first()],
    ],
    clicks: [
      [
        'filters-button',
        (p) => p.getByRole('button', { name: 'Filters' }),
        'click-filters',
        escape,
      ],
      [
        'board-card',
        (p) => p.locator('[data-task-id="1"]').first(),
        'click-card',
      ],
    ],
  },
  {
    name: 'list',
    url: '/demo/list',
    ready: (p) => p.locator('[data-task-id="1"]').first(),
    shots: [['tab-row', (p) => p.locator('[data-task-id="1"]').first()]],
    clicks: [
      ['search-input', (p) => textInput(p.locator('main')), 'click-input'],
      ['list-row', (p) => p.locator('[data-task-id="1"]').first(), 'click-row'],
    ],
  },
  {
    name: 'settings',
    url: '/settings/workspaces',
    ready: (p) => p.locator('[data-page-header] h1').first(),
    shots: [
      ['tab-save', (p) => p.getByRole('button', { name: 'Save' })],
      ['tab-select', (p) => p.getByRole('combobox')],
    ],
    clicks: [
      [
        'workspace-select',
        (p) => p.getByRole('combobox').first(),
        'click-select',
        escape,
      ],
      [
        'new-workspace-button',
        (p) => p.getByRole('button', { name: 'New workspace' }),
        'click-button',
        escape,
      ],
    ],
  },
  {
    name: 'member-page',
    url: '/demo/team/2',
    ready: (p) => p.getByRole('tab', { name: 'Assigned' }),
    shots: [['tab-tabs', (p) => p.getByRole('tab', { name: 'Assigned' })]],
    clicks: [
      [
        'member-tab',
        (p) => p.getByRole('tab', { name: 'Assigned' }),
        'click-tab',
      ],
      [
        'period-button',
        (p) => p.getByRole('button', { name: '30 days' }),
        'click-period',
      ],
    ],
  },
  {
    name: 'routines',
    url: '/routines',
    ready: (p) => p.getByText('Stretch', { exact: true }).first(),
    shots: [
      ['tab-week', (p) => p.getByRole('button', { name: 'Week', exact: true })],
    ],
    clicks: [
      [
        'week-button',
        (p) => p.getByRole('button', { name: 'Week', exact: true }),
        'click-week',
      ],
      ['search-input', (p) => textInput(p), 'click-input'],
      [
        'routine-entry',
        (p) => p.getByText('Stretch', { exact: true }).first(),
        'click-entry',
        escape,
      ],
    ],
  },
];

for (const scheme of ['light', 'dark']) {
  for (const pg of PAGES) {
    test(`${pg.name} ${scheme}: Tab shows the one inset style, mouse click shows none`, async ({
      page,
    }) => {
      test.setTimeout(180_000);
      await setup(page, scheme);
      await open(page, pg.url, pg.ready, scheme);
      const tag = `${pg.name}-${scheme}`;
      const ref = await reference(page);
      const stops = await walk(page, { shots: pg.shots, tag });
      for (const [label, locate, file, after] of pg.clicks) {
        await mouseProbe(page, locate(page), `${tag} ${label}`, {
          stops,
          ref,
          file: file && `${tag}-${file}`,
        });
        if (after) {
          await after(page);
          await page.waitForTimeout(300);
          // Escape is a keyboard action, so a ring on the restored focus is
          // expected (D-001); if one is drawn it must be the one style.
          const back = await page.evaluate(() => window.__qaFocus.probe());
          if (back?.outlineDrawn)
            expect
              .soft(
                back.kind === 'control'
                  ? tupleOf(back)
                  : `${back.kind} ${back.el}`,
                `${tag} ${label}: focus restored by Escape shows the one style`,
              )
              .toBe(ref.tuple);
          await page.evaluate(() => document.activeElement?.blur());
        }
      }
      if (pg.check) await pg.check(page, tag);
      expectKeyboardStops(stops, ref, tag);
    });
  }

  test(`sidebar ${scheme}: nav items show the one style on Tab, none on click`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await setup(page, scheme);
    await open(
      page,
      '/demo/list',
      (p) => p.locator('[data-task-id="1"]').first(),
      scheme,
    );
    const ref = await reference(page);
    const sidebar = page.locator('[data-sidebar="sidebar"]').first();
    const stops = await walk(page, {
      shots: [
        [
          'tab-board-link',
          (p) => p.getByRole('link', { name: 'Board', exact: true }),
        ],
      ],
      tag: `sidebar-${scheme}`,
    });
    const inSidebar = [];
    for (const s of stops)
      if (await sidebar.locator(`[data-qa-id="${s.qaId}"]`).count())
        inSidebar.push(s);
    const toggle = page.locator('[data-tour="nav.menu-button"]');
    await expect(toggle).toHaveCount(1);
    await mouseProbe(page, toggle, `sidebar-${scheme} toggle`);
    await mouseProbe(page, toggle, `sidebar-${scheme} toggle back`);
    await mouseProbe(
      page,
      page.getByRole('link', { name: 'Board', exact: true }).first(),
      `sidebar-${scheme} board link`,
      { file: `sidebar-${scheme}-click-board-link` },
    );
    expectKeyboardStops(inSidebar, ref, `sidebar-${scheme}`);
  });

  test(`task modal ${scheme}: Tab inside the modal shows the one style, clicks show none`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await setup(page, scheme);
    await open(
      page,
      '/demo/list',
      (p) => p.locator('[data-task-id="1"]').first(),
      scheme,
    );
    const ref = await reference(page);
    await page.locator('[data-task-id="1"]').first().click();
    const form = page.locator('.new-form-container');
    await expect(form).toBeVisible();
    await page.waitForTimeout(800);
    await shot(page, `task-modal-${scheme}-click-open`);
    const stops = await walk(page, {
      shots: [['tab-first-button', () => form.locator('button:visible')]],
      tag: `task-modal-${scheme}`,
    });
    const inModal = [];
    for (const s of stops)
      if (await form.locator(`[data-qa-id="${s.qaId}"]`).count())
        inModal.push(s);
    await mouseProbe(
      page,
      textInput(form),
      `task-modal-${scheme} title input`,
      { stops, ref, file: `task-modal-${scheme}-click-input` },
    );
    await mouseProbe(
      page,
      form.locator('button:visible').first(),
      `task-modal-${scheme} first button`,
      { file: `task-modal-${scheme}-click-button` },
    );
    expectKeyboardStops(inModal, ref, `task-modal-${scheme}`);
  });

  test(`members modal ${scheme}: Tab cycles member cards with the one style, click shows none`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await setup(page, scheme);
    await open(
      page,
      '/demo/board',
      (p) => p.locator('[data-task-id="1"]').first(),
      scheme,
    );
    const ref = await reference(page);
    const opener = page
      .locator('button:has(span[title="Ann Lee"]):visible')
      .first();
    await mouseProbe(page, opener, `members-modal-${scheme} opener`);
    const dialog = page.getByRole('dialog', { name: 'Workspace Members' });
    await expect(dialog).toBeVisible();
    await dialog.evaluate((el) =>
      Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)),
    );
    await shot(page, `members-modal-${scheme}-click-open`);
    const stops = await walk(page, {
      shots: [
        ['tab-member', () => dialog.getByRole('button', { name: /Ann Lee/ })],
      ],
      tag: `members-modal-${scheme}`,
    });
    await mouseProbe(
      page,
      dialog.getByText('Ann Lee', { exact: true }).first(),
      `members-modal-${scheme} member card`,
      { file: `members-modal-${scheme}-click-member` },
    );
    expectKeyboardStops(stops, ref, `members-modal-${scheme}`);
  });

  test(`dropdown ${scheme}: trigger shows the one style, menu items keep the background highlight`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await setup(page, scheme);
    await open(
      page,
      '/demo/list',
      (p) => p.locator('[data-task-id="1"]').first(),
      scheme,
    );
    const ref = await reference(page);
    const trigger = page
      .locator('[data-task-id="1"]')
      .first()
      .getByRole('button', { name: 'Task actions', exact: true });
    await mouseProbe(page, trigger, `dropdown-${scheme} trigger click`);
    const menu = page.getByRole('menu');
    await expect(menu).toBeVisible();
    const items = menu.getByRole('menuitem');
    const first = await items.first().boundingBox();
    await page.mouse.move(
      first.x + first.width / 2,
      first.y + first.height / 2,
    );
    await page.waitForTimeout(300);
    await shot(page, `dropdown-${scheme}-hover-item`);
    const hovered = await page.evaluate(() => window.__qaFocus.probe());
    expect
      .soft(
        hovered?.outlineDrawn,
        `dropdown-${scheme}: hovered item draws no outline`,
      )
      .toBe(false);
    await escape(page);
    await expect(menu).toBeHidden();
    await page.waitForTimeout(300);
    await shot(page, `dropdown-${scheme}-escape-trigger`);
    const back = await page.evaluate(() => window.__qaFocus.probe());
    expect
      .soft(
        back && tupleOf(back),
        `dropdown-${scheme}: focus back on the trigger after Escape shows the one style`,
      )
      .toBe(ref.tuple);
    await page.keyboard.press('Enter');
    await expect(menu).toBeVisible();
    await page.keyboard.press('ArrowDown');
    await page.waitForTimeout(300);
    await shot(page, `dropdown-${scheme}-keyboard-item`);
    const item = await page.evaluate(() => window.__qaFocus.probe());
    expect
      .soft(item?.kind, `dropdown-${scheme}: keyboard focus is on a menu item`)
      .toBe('menu');
    expect
      .soft(
        item?.outlineDrawn,
        `dropdown-${scheme}: keyboard-focused item draws no outline (D-005)`,
      )
      .toBe(false);
    const others = await items.evaluateAll((els) =>
      els
        .filter((e) => e !== document.activeElement)
        .map((e) => getComputedStyle(e).backgroundColor),
    );
    expect
      .soft(
        others,
        `dropdown-${scheme}: focused item background differs from the others`,
      )
      .not.toContain(item?.background);
  });

  // D-007: the Ctrl/Cmd+1..4 hotkey collides with workspace switching (App.vue,
  // separate ticket), so the sections are focused by script after a keypress.
  test(`dashboard sections ${scheme}: not in Tab order, programmatic focus draws no outline`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await setup(page, scheme);
    await open(
      page,
      '/demo/dashboard',
      (p) => p.locator('[aria-labelledby="statistics-heading"]'),
      scheme,
    );
    const sections = [
      'statistics-heading',
      'heatmap-heading',
      'activity-heading',
      'team-activity-heading',
    ];
    const stops = await walk(page, {
      max: 80,
      tag: `dashboard-sections-${scheme}`,
    });
    for (const heading of sections) {
      const target = page.locator(`[aria-labelledby="${heading}"]`);
      await expect
        .soft(target, `${heading} section is a programmatic focus target`)
        .toHaveAttribute('tabindex', '-1');
      await page.keyboard.press('Shift');
      await target.evaluate((el) => el.focus());
      await expect.soft(target, `${heading} takes focus`).toBeFocused();
      await shot(page, `dashboard-${scheme}-focus-${heading}`);
      const s = await page.evaluate(() => window.__qaFocus.probe());
      expect
        .soft(
          s?.focusVisible,
          `${heading}: keyboard-modality focus matches :focus-visible`,
        )
        .toBe(true);
      expect
        .soft(s?.outlineDrawn, `${heading}: section draws no outline (D-004)`)
        .toBe(false);
      expect
        .soft(
          [...(s?.decorations ?? []), s?.ownShadow].filter(Boolean),
          `${heading}: section has no focus ring or band`,
        )
        .toEqual([]);
    }
    expect
      .soft(
        stops
          .filter((s) => s.tag === 'section' || s.role === 'region')
          .map((s) => s.el),
        `dashboard-${scheme}: sections/regions in the Tab order`,
      )
      .toEqual([]);
  });

  for (const [state, opts] of [
    ['empty', { empty: true }],
    ['error', { empty: true, failStatistics: true }],
  ])
    test(`dashboard ${state} ${scheme}: Tab order only interactive, the one style on error/empty controls`, async ({
      page,
    }) => {
      test.setTimeout(180_000);
      await setup(page, scheme, opts);
      await open(
        page,
        '/demo/dashboard',
        (p) => p.locator('[aria-labelledby="statistics-heading"]'),
        scheme,
      );
      if (state === 'error')
        await expect(page.getByText('Dashboard Error')).toBeVisible({
          timeout: 45_000,
        });
      const ref = await reference(page);
      const stops = await walk(page, {
        max: 80,
        shots: [
          [
            'tab-try-again',
            (p) => p.getByRole('button', { name: 'Try Again' }),
          ],
        ],
        tag: `dashboard-${state}-${scheme}`,
      });
      expectKeyboardStops(stops, ref, `dashboard-${state}-${scheme}`);
    });

  test(`dashboard offline ${scheme}: Check Connection shows the one style on Tab, none on click`, async ({
    page,
    context,
  }) => {
    test.setTimeout(180_000);
    await setup(page, scheme, { empty: true });
    await open(
      page,
      '/demo/dashboard',
      (p) => p.locator('[aria-labelledby="statistics-heading"]'),
      scheme,
    );
    const ref = await reference(page);
    await context.setOffline(true);
    const check = page.getByRole('button', { name: 'Check Connection' });
    await expect(check).toBeVisible();
    await check.focus();
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Tab');
    await shot(page, `dashboard-offline-${scheme}-tab-check-connection`);
    const s = await page.evaluate(() => window.__qaFocus.probe());
    await mouseProbe(
      page,
      check,
      `dashboard-offline-${scheme} check connection`,
      { file: `dashboard-offline-${scheme}-click-check-connection` },
    );
    await context.setOffline(false);
    expectKeyboardStops(
      [s].filter(Boolean),
      ref,
      `dashboard-offline-${scheme}`,
      { min: 1 },
    );
  });

  test(`map timeline ${scheme}: Tab onto Play and the scrubber shows the one style, click shows none`, async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.addInitScript((sc) => {
      sessionStorage.setItem('tmgr:tours:auto', 'true');
      localStorage.setItem('colorScheme', sc);
    }, scheme);
    await page.addInitScript(installProbe);
    await setupMap(page, buildGraphMap());
    await openMap(page);
    if (scheme === 'dark')
      await expect(page.locator('html')).toHaveClass(/\bdark\b/);
    else await expect(page.locator('html')).not.toHaveClass(/\bdark\b/);
    const ref = await reference(page);
    const play = page.getByTestId('map-play');
    const scrubber = page.getByTestId('map-scrubber');
    await scrubber.focus();
    await page.keyboard.press('Shift+Tab');
    await expect(play).toBeFocused();
    await shot(page, `map-${scheme}-tab-play`);
    const p = await page.evaluate(() => window.__qaFocus.probe());
    // D-010: on the ink-filled Play button the outline matches the fill, so
    // the inner band must differ from the background to keep the ring visible.
    const band = layerColor(firstLayer(p?.ownShadow));
    expect.soft(band, `map-${scheme}: Play has the inner band`).not.toBeNull();
    expect
      .soft(band, `map-${scheme}: band color differs from the Play fill`)
      .not.toBe(p?.background);
    await page.keyboard.press('Tab');
    await expect(scrubber).toBeFocused();
    await shot(page, `map-${scheme}-tab-scrubber`);
    const s = await page.evaluate(() => window.__qaFocus.probe());
    await mouseProbe(page, scrubber, `map-${scheme} scrubber`, {
      file: `map-${scheme}-click-scrubber`,
    });
    await mouseProbe(page, play, `map-${scheme} play`, {
      file: `map-${scheme}-click-play`,
    });
    expectKeyboardStops([p, s].filter(Boolean), ref, `map-${scheme}`, {
      min: 2,
    });
  });

  for (const [menuName, locate] of [
    [
      'board column menu',
      (p) =>
        p
          .locator('div:not([data-tour="statuses.editor"]) > div > button', {
            hasText: 'Open options',
          })
          .first(),
    ],
    [
      'board actions menu',
      (p) =>
        p.locator('[data-tour="statuses.editor"] button', {
          hasText: 'Open options',
        }),
    ],
  ])
    test(`${menuName} ${scheme}: keyboard-opened Headless UI panel draws no outline, the active item is highlighted`, async ({
      page,
    }) => {
      test.setTimeout(180_000);
      await setup(page, scheme);
      await open(
        page,
        '/demo/board',
        (p) => p.locator('[data-task-id="1"]').first(),
        scheme,
      );
      const ref = await reference(page);
      const trigger = locate(page);
      await expect(trigger).toHaveCount(1);
      await trigger.focus();
      const t = await page.evaluate(() => window.__qaFocus.probe());
      await page.keyboard.press('Enter');
      const menu = page.getByRole('menu');
      await expect(menu).toBeVisible();
      await page.keyboard.press('ArrowDown');
      await page.waitForTimeout(300);
      await shot(page, `${menuName}-${scheme}-keyboard-item`);
      const panel = await page.evaluate(() => window.__qaFocus.probe());
      expect
        .soft(panel?.role, `${menuName}-${scheme}: focus is on the menu panel`)
        .toBe('menu');
      expect
        .soft(
          [
            panel?.outlineDrawn && `outline ${tupleOf(panel)}`,
            panel?.ownShadow && `box-shadow ${panel.ownShadow}`,
            ...(panel?.decorations ?? []),
          ].filter(Boolean),
          `${menuName}-${scheme}: [role=menu] panel draws no ring (D-012)`,
        )
        .toEqual([]);
      const highlight = await menu.evaluate((m) => {
        const id = m.getAttribute('aria-activedescendant');
        const items = [...m.querySelectorAll('[role=menuitem]')];
        const bg = (e) => getComputedStyle(e).backgroundColor;
        const active = items.find((e) => e.id === id);
        return {
          active: active ? bg(active) : null,
          others: items.filter((e) => e !== active).map(bg),
        };
      });
      expect
        .soft(highlight.active, `${menuName}-${scheme}: an item is active`)
        .not.toBeNull();
      expect
        .soft(
          highlight.others,
          `${menuName}-${scheme}: active item background differs from the others (D-005)`,
        )
        .not.toContain(highlight.active);
      await escape(page);
      await expect(menu).toBeHidden();
      await page.waitForTimeout(300);
      const back = await page.evaluate(() => window.__qaFocus.probe());
      expectKeyboardStops(
        [t, back].filter(Boolean),
        ref,
        `${menuName}-${scheme}`,
        { min: 2 },
      );
    });

  for (const palette of scheme === 'dark' ? [null, 'nord'] : [null])
    test(`global search ${
      palette ?? scheme
    }: Tab and click give the input the --focus-border color`, async ({
      page,
    }) => {
      test.setTimeout(180_000);
      await setup(page, scheme);
      await open(
        page,
        '/demo/list',
        (p) => p.locator('[data-task-id="1"]').first(),
        scheme,
      );
      if (palette)
        expect(
          await page.evaluate((c) => {
            const fg = () =>
              getComputedStyle(document.documentElement).getPropertyValue(
                '--fg',
              );
            const before = fg();
            document.documentElement.classList.add(c);
            return before !== fg();
          }, `theme-${palette}`),
          `theme-${palette} repaints --fg`,
        ).toBe(true);
      const tag = `global-search-${palette ?? scheme}`;
      const ref = await reference(page);
      await page
        .getByRole('button', { name: 'Search', exact: true })
        .first()
        .click();
      const dialog = page.getByRole('dialog', { name: 'Search' });
      await expect(dialog).toBeVisible();
      await dialog.evaluate((el) =>
        Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)),
      );
      const input = dialog.locator('input').first();
      await input.focus();
      await page.keyboard.press('Shift+Tab');
      await page.keyboard.press('Tab');
      await expect(input).toBeFocused();
      await shot(page, `${tag}-tab-input`);
      const s = await page.evaluate(() => window.__qaFocus.probe());
      if (palette) {
        expect
          .soft(
            await page.evaluate(
              (c) => document.documentElement.classList.contains(c),
              `theme-${palette}`,
            ),
            `${tag}: palette applied`,
          )
          .toBe(true);
        expect
          .soft(s?.focusBorderColor, `${tag}: focus border differs from rest`)
          .not.toBe(s?.blurredBorder);
      }
      await mouseProbe(page, input, tag, {
        stops: [s].filter(Boolean),
        ref,
        file: `${tag}-click-input`,
      });
      expectKeyboardStops([s].filter(Boolean), ref, tag, { min: 1 });
    });
}
