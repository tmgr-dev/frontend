import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

// TM-701: heatmap range select. All dates are UTC and derived from "today", so the spec does not age.
const SHOTS = process.env.HEATMAP_SHOTS_DIR;
const DAY = 86_400_000;
const STORAGE_KEY = 'tmgr:dashboard:heatmap-range:1:1';
const todayMs = () => {
  const now = new Date();
  return Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
};
const key = (ms) => new Date(ms).toISOString().slice(0, 10);
const dayKey = (back) => key(todayMs() - back * DAY);
const TODAY = dayKey(0);
const CUR = Number(TODAY.slice(0, 4));
const YEARS = [CUR, CUR - 1, CUR - 2];
const ROLLING_FROM = `${CUR - 1}${TODAY.slice(4)}`;

const days = (from, to) => {
  const out = [];
  for (let t = Date.parse(from); t <= Date.parse(to); t += DAY)
    out.push(key(t));
  return out;
};

// Rolling: 2 a day this week, 1 a day before (60 days) → KPI 7d 14 (+100%), 30d 37 (+23%), as in dashboard-trends.
const rolling = ({ legacy = false, empty = false } = {}) => {
  const contributions = days(ROLLING_FROM, TODAY).map((date) => {
    const back = Math.round((todayMs() - Date.parse(date)) / DAY);
    const count = empty ? 0 : back < 7 ? 2 : back < 60 ? 1 : 0;
    return { date, count, level: count };
  });
  const base = {
    contributions,
    total_contributions: contributions.reduce((s, c) => s + c.count, 0),
    streak: { current: 5, longest: 12 },
    year: CUR,
    weeks: 53,
  };
  return legacy
    ? base
    : { ...base, from: ROLLING_FROM, to: TODAY, years: YEARS };
};

// A calendar year: 3 a day every day up to min(Y-12-31, today).
const calendarYear = (y) => {
  const from = `${y}-01-01`;
  const to = y === CUR ? TODAY : `${y}-12-31`;
  const contributions = days(from, to).map((date) => ({
    date,
    count: 3,
    level: 2,
  }));
  return {
    contributions,
    total_contributions: contributions.length * 3,
    streak: { current: 4, longest: 11 },
    year: y,
    weeks: 53,
    from,
    to,
    years: YEARS,
  };
};

const teamActivity = (window) => ({
  members: [],
  total_members: 2,
  online_members: 1,
  active_today: 1,
  active_timers: 0,
  tracked_seconds: 3600,
  done_count: window === '30d' ? 12 : 5,
  window,
});

const setup = async (
  page,
  {
    legacy = false,
    empty = false,
    yearStatus = () => 200,
    yearDelay = 0,
    dark = false,
  } = {},
) => {
  const heatmapRequests = [];
  const activityDates = [];
  const errors = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() === 'error' && !m.text().startsWith('Failed to load resource'))
      errors.push(m.text());
  });
  await page.addInitScript((dark) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    if (dark) localStorage.setItem('colorScheme', 'dark');
  }, dark);
  await mockApp(page, { taskCount: 1 });
  await page.route('**/api/workspaces/1/modules', (r) =>
    r.fulfill({
      json: {
        data: {
          modules: ['board', 'dashboard', 'exports', 'task.countdown'].map(
            (key) => ({ key, enabled: true }),
          ),
        },
      },
    }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard/'),
    async (r) => {
      const url = new URL(r.request().url());
      const p = url.pathname;
      if (p.endsWith('heatmap')) {
        const params = Object.fromEntries(url.searchParams);
        heatmapRequests.push(params);
        if (params.range !== 'year')
          return r.fulfill({ json: { data: rolling({ legacy, empty }) } });
        const y = Number(params.year);
        if (yearDelay) await new Promise((res) => setTimeout(res, yearDelay));
        const status = yearStatus(y);
        if (status !== 200)
          return r.fulfill({ status, json: { message: 'boom' } });
        return r.fulfill({ json: { data: calendarYear(y) } });
      }
      if (p.endsWith('team-activity'))
        return r.fulfill({
          json: { data: teamActivity(url.searchParams.get('window')) },
        });
      if (p.endsWith('activities')) {
        activityDates.push(url.searchParams.get('date'));
        return r.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      }
      return r.fulfill({ json: { data: {} } });
    },
  );
  await page.route('**/api/workspaces/1/time/daily**', (r) => {
    const url = new URL(r.request().url());
    const from = url.searchParams.get('from');
    const to = url.searchParams.get('to');
    return r.fulfill({ json: { data: { from, to, tz: 'UTC', days: [] } } });
  });
  const yearRequests = () =>
    heatmapRequests.filter((q) => q.range === 'year').map((q) => q.year);
  return { heatmapRequests, yearRequests, activityDates, errors };
};

const total = (page) => page.getByTestId('heatmap-total');
const streak = (page) => page.getByTestId('heatmap-streak');
const trigger = (page) => page.getByTestId('heatmap-range-select');
const card = (page) =>
  page.locator('section[aria-labelledby="heatmap-heading"]');
const cellDates = (page) =>
  page
    .locator('.heatmap-calendar [data-date]')
    .evaluateAll((els) => els.map((el) => el.getAttribute('data-date')));
const kpi = (page, k) => page.locator(`[data-kpi="${k}"]`);

const expectGrid = async (page, from, to) => {
  await expect
    .poll(async () => {
      const d = await cellDates(page);
      return [d[0], d[d.length - 1], d.length];
    })
    .toEqual([from, to, days(from, to).length]);
  const d = await cellDates(page);
  expect([...d].sort()).toEqual(d);
  expect(d.filter((x) => x > to)).toEqual([]);
};

const choose = async (page, name) => {
  await trigger(page).click();
  await page.getByRole('option', { name, exact: true }).click();
};

const shot = async (target, name) => {
  if (SHOTS) await target.screenshot({ path: `${SHOTS}/${name}.png` });
};
const listboxSettled = (page) =>
  page
    .getByRole('listbox')
    .evaluate((el) =>
      Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)),
    );

const open = async (page, opts) => {
  const ctx = await setup(page, opts);
  await page.goto('/demo/dashboard');
  await expect(total(page)).toBeVisible();
  return ctx;
};

test.setTimeout(90_000);
test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
});

test('default view: rolling range, copy, select options newest first', async ({
  page,
}) => {
  const { heatmapRequests, yearRequests, errors } = await open(page);
  const sum = rolling().total_contributions;
  await expect(total(page)).toHaveText(`${sum} contributions in the last year`);
  await expect(streak(page)).toHaveText(
    'Current streak: 5 days • Longest: 12 days',
  );
  await expect(page.locator('#heatmap-description')).toHaveText(
    'Activity of everyone in this workspace. Each square is one day (UTC); darker squares mean more activity.',
  );
  await expect(page.getByText('Your contribution')).toHaveCount(0);
  await expectGrid(page, ROLLING_FROM, TODAY);

  await expect(trigger(page)).toHaveAccessibleName('Heatmap range');
  await expect(trigger(page)).toContainText('Last 12 months');
  await shot(card(page), 'light-default');
  await trigger(page).click();
  await expect(page.getByRole('option')).toHaveText([
    'Last 12 months',
    ...YEARS.map(String),
  ]);
  await listboxSettled(page);
  await shot(page, 'light-select-open');
  await page.keyboard.press('Escape');

  expect(heatmapRequests.length).toBeGreaterThan(0);
  expect(yearRequests()).toEqual([]);
  expect(errors).toEqual([]);
});

test('a past year: request, headline, grid Jan 1..Dec 31, streak copy, day click, survives reload', async ({
  page,
}) => {
  const Y = CUR - 1;
  const { yearRequests, activityDates, errors } = await open(page);
  await choose(page, String(Y));

  await expect(total(page)).toHaveText(
    `${calendarYear(Y).total_contributions} contributions in ${Y}`,
  );
  expect(yearRequests()).toEqual([String(Y)]);
  await expect(streak(page)).toHaveText(`Longest streak in ${Y}: 11 days`);
  await expectGrid(page, `${Y}-01-01`, `${Y}-12-31`);
  await expect(page.locator('.heatmap-calendar').getByText('Jan')).toHaveCount(
    1,
  );
  await expect(trigger(page)).toContainText(String(Y));
  expect(await page.evaluate((k) => localStorage.getItem(k), STORAGE_KEY)).toBe(
    String(Y),
  );
  await shot(card(page), 'light-past-year');

  const date = `${Y}-03-15`;
  const req = page.waitForRequest((r) => {
    const u = new URL(r.url());
    return (
      u.pathname.endsWith('/dashboard/activities') &&
      u.searchParams.get('date') === date
    );
  });
  await page.locator(`.heatmap-calendar [data-date="${date}"]`).click();
  await req;
  await expect(
    page.locator(`.heatmap-calendar [data-date="${date}"]`),
  ).toHaveAttribute('aria-pressed', 'true');
  expect(activityDates).toContain(date);

  await page.reload();
  await expect(total(page)).toHaveText(
    `${calendarYear(Y).total_contributions} contributions in ${Y}`,
  );
  await expect(trigger(page)).toContainText(String(Y));
  expect(yearRequests()).toEqual([String(Y), String(Y)]);
  expect(errors).toEqual([]);
});

test('the current year: grid ends today, current streak copy', async ({
  page,
}) => {
  const { yearRequests, errors } = await open(page);
  await choose(page, String(CUR));
  await expect(total(page)).toHaveText(
    `${calendarYear(CUR).total_contributions} contribution${
      calendarYear(CUR).total_contributions === 1 ? '' : 's'
    } in ${CUR}`,
  );
  expect(yearRequests()).toEqual([String(CUR)]);
  await expect(streak(page)).toHaveText(
    'Current streak: 4 days • Longest: 11 days',
  );
  await expectGrid(page, `${CUR}-01-01`, TODAY);

  await choose(page, 'Last 12 months');
  await expect(total(page)).toContainText('in the last year');
  await expectGrid(page, ROLLING_FROM, TODAY);
  expect(await page.evaluate((k) => localStorage.getItem(k), STORAGE_KEY)).toBe(
    'last_12_months',
  );
  expect(yearRequests()).toEqual([String(CUR)]);
  expect(errors).toEqual([]);
});

test('KPI trends keep the rolling data when a year is selected', async ({
  page,
}) => {
  const { errors } = await open(page);
  const delta = (k) => kpi(page, k).locator('[data-kpi-delta]');
  await expect(kpi(page, 'activity')).toContainText('14');
  await expect(delta('activity')).toContainText('+100%');
  const before = await kpi(page, 'activity').innerText();

  // The current year overlaps the KPI window, so leaking its data would change 14 → 21.
  await choose(page, String(CUR));
  await expect(total(page)).toContainText(`in ${CUR}`);
  expect(await kpi(page, 'activity').innerText()).toBe(before);

  await page.getByRole('button', { name: 'Last 30 days' }).click();
  await expect(kpi(page, 'completed')).toContainText('12');
  await expect(kpi(page, 'activity')).toContainText('37');
  await expect(delta('activity')).toContainText('+23%');
  await expect(total(page)).toContainText(`in ${CUR}`);
  expect(errors).toEqual([]);
});

for (const stored of [String(CUR - 7), 'garbage'])
  test(`a stored value not in years (${stored}) falls back to Last 12 months`, async ({
    page,
  }) => {
    await page.addInitScript(
      ([k, v]) => localStorage.setItem(k, v),
      [STORAGE_KEY, stored],
    );
    const { yearRequests, errors } = await open(page);
    await expect(total(page)).toContainText('in the last year');
    await expect(trigger(page)).toContainText('Last 12 months');
    await page.waitForTimeout(500);
    expect(yearRequests()).toEqual([]);
    expect(errors).toEqual([]);
  });

test('old backend (no years/from/to): no select, no year request, grid drawn', async ({
  page,
}) => {
  await page.addInitScript(
    ([k, v]) => localStorage.setItem(k, v),
    [STORAGE_KEY, String(CUR - 1)],
  );
  const { yearRequests, errors } = await open(page, { legacy: true });
  await expect(total(page)).toHaveText(
    `${rolling().total_contributions} contributions in the last year`,
  );
  await expect(trigger(page)).toHaveCount(0);
  await expectGrid(page, ROLLING_FROM, TODAY);
  await page.waitForTimeout(500);
  expect(yearRequests()).toEqual([]);
  expect(errors).toEqual([]);
});

test('a failing year request shows the error card; Last 12 months and Retry recover', async ({
  page,
}) => {
  let failing = true;
  const Y = CUR - 1;
  const { yearRequests } = await open(page, {
    yearStatus: () => (failing ? 500 : 200),
  });
  await expect(kpi(page, 'activity')).toContainText('14');
  const before = await kpi(page, 'activity').innerText();
  await choose(page, String(Y));

  await expect(page.getByText('Failed to load heatmap data')).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible();
  // D-009: no auto-retry for the year request (axios retries 5xx unless told otherwise).
  await page.waitForTimeout(3000);
  expect(yearRequests()).toEqual([String(Y)]);
  expect(await kpi(page, 'activity').innerText()).toBe(before);

  await choose(page, 'Last 12 months');
  await expect(total(page)).toContainText('in the last year');
  await expect(page.getByText('Failed to load heatmap data')).toHaveCount(0);

  await choose(page, String(Y));
  await expect(page.getByText('Failed to load heatmap data')).toBeVisible({
    timeout: 15_000,
  });
  failing = false;
  await page.getByRole('button', { name: 'Retry' }).click();
  await expect(total(page)).toContainText(`in ${Y}`);
  await expectGrid(page, `${Y}-01-01`, `${Y}-12-31`);
});

test('reopening the dashboard with a year selected never shows mismatched data', async ({
  page,
}) => {
  const Y = CUR - 1;
  await open(page, { yearDelay: 800 });
  await choose(page, String(Y));
  await expect(total(page)).toContainText(`in ${Y}`);

  await page.evaluate(() => {
    window.__heatmapFrames = [];
    const record = () => {
      const t = document.querySelector('[data-testid="heatmap-total"]');
      const first = document.querySelector('.heatmap-calendar [data-date]');
      if (t)
        window.__heatmapFrames.push([
          t.textContent.trim(),
          first?.dataset.date,
        ]);
    };
    new MutationObserver(record).observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: ['data-date'],
    });
  });
  const push = (path) =>
    page.evaluate(
      (path) =>
        document
          .querySelector('#app')
          .__vue_app__.config.globalProperties.$router.push(path),
      path,
    );
  await push('/demo/board');
  await expect(total(page)).toHaveCount(0);
  await push('/demo/dashboard');
  await expect(total(page)).toContainText(`in ${Y}`);
  await page.waitForTimeout(1500);
  const frames = await page.evaluate(() => window.__heatmapFrames);
  expect(frames.length).toBeGreaterThan(0);
  for (const [text, first] of frames) {
    if (text.endsWith(`in ${Y}`)) expect(first).toBe(`${Y}-01-01`);
    if (text.endsWith('in the last year')) expect(first).toBe(ROLLING_FROM);
  }
  await expect(total(page)).toContainText(`in ${Y}`);
});

test('dark theme screenshots: default, year, select open', async ({ page }) => {
  const { errors } = await open(page, { dark: true });
  await expect(page.locator('html')).toHaveClass(/dark/);
  await expect(total(page)).toContainText('in the last year');
  await shot(card(page), 'dark-default');
  await choose(page, String(CUR - 1));
  await expect(total(page)).toContainText(`in ${CUR - 1}`);
  await shot(card(page), 'dark-past-year');
  await trigger(page).click();
  await expect(page.getByRole('option')).toHaveCount(4);
  await listboxSettled(page);
  await shot(page, 'dark-select-open');
  await page.keyboard.press('Escape');
  expect(errors).toEqual([]);
});

test('narrow 390px: card header keeps the select inside the card', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const warnings = [];
  page.on('console', (m) => {
    if (m.text().includes('Invalid prop')) warnings.push(m.text());
  });
  const { errors } = await open(page);
  await trigger(page).scrollIntoViewIfNeeded();
  const box = async (l) => l.boundingBox();
  const c = await box(card(page));
  const t = await box(trigger(page));
  const h = await box(page.locator('#heatmap-heading'));
  expect(t.x).toBeGreaterThanOrEqual(c.x);
  expect(t.x + t.width).toBeLessThanOrEqual(c.x + c.width + 0.5);
  const overlap =
    t.x < h.x + h.width &&
    h.x < t.x + t.width &&
    t.y < h.y + h.height &&
    h.y < t.y + t.height;
  expect(overlap).toBe(false);
  const overflow = await page.evaluate(
    () =>
      document.documentElement.scrollWidth -
      document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
  // TM-712: the grid opens scrolled to the most recent days.
  const scroller = card(page).locator('.heatmap-calendar .overflow-x-auto');
  const gapToEnd = () =>
    scroller.evaluate((el) => el.scrollWidth - el.clientWidth - el.scrollLeft);
  expect(await scroller.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(
    true,
  );
  await expect.poll(gapToEnd).toBeLessThanOrEqual(1);
  await shot(card(page), 'light-390');
  await choose(page, String(CUR - 1));
  await expect(total(page)).toContainText(`in ${CUR - 1}`);
  await expect.poll(gapToEnd).toBeLessThanOrEqual(1);
  await shot(card(page), 'light-390-past-year');
  expect(errors).toEqual([]);
  expect(warnings).toEqual([]);
});

// TM-735: weekday labels share the day grid's rows; the chart is one Tab stop with arrow-key navigation.
const stop = (page) =>
  page.locator('.heatmap-calendar [data-date][tabindex="0"]');
const cell = (page, date) =>
  page.locator(`.heatmap-calendar [data-date="${date}"]`);
const tabStops = (page) =>
  page.evaluate(() => {
    const cells = [
      ...document.querySelectorAll('.heatmap-calendar [data-date]'),
    ];
    return {
      total: cells.length,
      tabbable: cells
        .filter((el) => el.getAttribute('tabindex') === '0')
        .map((el) => el.dataset.date),
      rovers: cells.filter((el) => el.getAttribute('tabindex') === '-1').length,
    };
  });

for (const width of [1440, 390])
  test(`weekday labels sit in the same rows as Monday, Wednesday and Friday cells at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1000 });
    const { errors } = await open(page);
    await expectGrid(page, ROLLING_FROM, TODAY);
    const rows = await page.evaluate(() => {
      const centre = (el) => {
        const r = el.getBoundingClientRect();
        return r.top + r.height / 2;
      };
      const labels = [
        ...document.querySelectorAll(
          '.heatmap-calendar [data-testid="heatmap-weekdays"] > div',
        ),
      ];
      const week = document.querySelectorAll('.heatmap-calendar [data-date]')[7]
        .parentElement;
      const cellRows = [0, 2, 4].map((row) => week.children[row]);
      const cell = week.children[0].getBoundingClientRect();
      return {
        weekHeight: week.getBoundingClientRect().height,
        cellsHeight: 7 * cell.height + 6 * 2,
        labelsHeight: labels[0].parentElement.getBoundingClientRect().height,
        text: labels.map((l) => l.textContent.trim()),
        offsets: labels.map((l, i) =>
          Math.abs(centre(l) - centre(cellRows[i])),
        ),
        weekdays: cellRows.map((c) =>
          new Date(`${c.dataset.date}T00:00:00Z`).getUTCDay(),
        ),
      };
    });
    expect(rows.text).toEqual(['Mon', 'Wed', 'Fri']);
    expect(rows.weekdays).toEqual([1, 3, 5]);
    expect(Math.abs(rows.weekHeight - rows.cellsHeight)).toBeLessThanOrEqual(
      0.5,
    );
    expect(Math.abs(rows.labelsHeight - rows.weekHeight)).toBeLessThanOrEqual(
      0.5,
    );
    for (const offset of rows.offsets) expect(offset).toBeLessThanOrEqual(1);
    await shot(card(page), `weekday-labels-${width}`);
    expect(errors).toEqual([]);
  });

test('one Tab stop (last day), arrows move by day and week, Enter and click select', async ({
  page,
}) => {
  const { activityDates, errors } = await open(page);
  await expectGrid(page, ROLLING_FROM, TODAY);
  const initial = await tabStops(page);
  expect(initial.tabbable).toEqual([TODAY]);
  expect(initial.rovers).toBe(initial.total - 1);

  await stop(page).focus();
  await expect(cell(page, TODAY)).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(cell(page, TODAY)).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(cell(page, dayKey(7))).toBeFocused();
  await expect(stop(page)).toHaveAttribute('data-date', dayKey(7));
  await page.keyboard.press('ArrowUp');
  await expect(cell(page, dayKey(8))).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await expect(cell(page, dayKey(6))).toBeFocused();
  expect((await tabStops(page)).tabbable).toEqual([dayKey(6)]);
  await shot(card(page), 'keyboard-focus');

  await page.keyboard.press('Enter');
  await expect(cell(page, dayKey(6))).toHaveAttribute('aria-pressed', 'true');
  expect(activityDates).toContain(dayKey(6));
  expect((await tabStops(page)).tabbable).toEqual([dayKey(6)]);

  await cell(page, dayKey(20)).click();
  await expect(cell(page, dayKey(20))).toHaveAttribute('aria-pressed', 'true');
  expect((await tabStops(page)).tabbable).toEqual([dayKey(20)]);
  expect(errors).toEqual([]);
});

test('with no activity there is still one Tab stop and arrows stop at the range edges', async ({
  page,
}) => {
  const { errors } = await open(page, { empty: true });
  await expectGrid(page, ROLLING_FROM, TODAY);
  const initial = await tabStops(page);
  expect(initial.tabbable).toEqual([TODAY]);
  expect(initial.rovers).toBe(initial.total - 1);

  await cell(page, ROLLING_FROM).focus();
  await page.keyboard.press('ArrowLeft');
  await expect(cell(page, ROLLING_FROM)).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await expect(cell(page, ROLLING_FROM)).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(
    cell(page, key(Date.parse(ROLLING_FROM) + 7 * DAY)),
  ).toBeFocused();
  expect(errors).toEqual([]);
});
