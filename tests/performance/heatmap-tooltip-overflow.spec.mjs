import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

test.use({ launchOptions: { ignoreDefaultArgs: ['--hide-scrollbars'] } });

const SQUARE = '.heatmap-calendar [style*="aspect-ratio"]';
const TOOLTIP_RE = /\d+ contributions? on [A-Z][a-z]{2} \d{1,2}, \d{4}/;

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

const mockDashboard = async (page) => {
  await page.route(
    (u) =>
      u.pathname.startsWith('/api/') &&
      u.pathname.endsWith('feature-toggles') &&
      !u.pathname.includes('/user'),
    (r) =>
      r.fulfill({
        json: {
          data: { board: { enabled: true }, dashboard: { enabled: true } },
        },
      }),
  );
  await page.route(
    (u) => u.pathname.startsWith('/api/') && u.pathname.includes('/dashboard/'),
    (r) => {
      const p = new URL(r.request().url()).pathname;
      if (p.endsWith('activities'))
        return r.fulfill({
          json: {
            data: [],
            meta: { current_page: 1, per_page: 20, total: 0, last_page: 1 },
            links: {},
          },
        });
      if (p.endsWith('heatmap'))
        return r.fulfill({ json: { data: heatmap() } });
      if (p.endsWith('team-activity'))
        return r.fulfill({
          json: {
            data: {
              members: [],
              total_members: 0,
              online_members: 0,
              active_today: 0,
              active_timers: 0,
              tracked_seconds: 0,
              done_count: 0,
              window: 'today',
            },
          },
        });
      if (p.endsWith('recent-tasks')) return r.fulfill({ json: { data: [] } });
      if (p.endsWith('statistics'))
        return r.fulfill({
          json: {
            data: {
              total_tasks: 3,
              active_tasks: 1,
              completed_today: 1,
              completed_week: 1,
              completed_month: 1,
              time_today: 1,
              time_week: 1,
              time_month: 1,
              time_total: 1,
              team_members: 1,
              overdue_tasks: 0,
              daily_routine_completion_rate: 10,
            },
          },
        });
      return r.fulfill({ json: { data: {} } });
    },
  );
};

const open = async (page, { width, height, scheme }) => {
  await page.setViewportSize({ width, height });
  await page.addInitScript((s) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    localStorage.setItem('colorScheme', s);
  }, scheme);
  await mockApp(page, { taskCount: 3 });
  await mockDashboard(page);
  await page.goto('/demo/dashboard');
  await page.locator(SQUARE).first().waitFor();
  if (scheme === 'dark')
    await expect(page.locator('html')).toHaveClass(/\bdark\b/);
  else await expect(page.locator('html')).not.toHaveClass(/\bdark\b/);
};

const settle = async (page) => {
  await page.evaluate(
    () =>
      new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );
  return page
    .waitForFunction(
      (sel) =>
        [...document.querySelectorAll(sel)].every(
          (s) => s.getAnimations().length === 0,
        ),
      SQUARE,
      { timeout: 3000 },
    )
    .then(
      () => true,
      () => false,
    );
};

const snap = (page, idx) =>
  page.evaluate(
    ({ sel, idx, re }) => {
      const rx = new RegExp(re);
      const squares = [...document.querySelectorAll(sel)];
      const sq = squares[idx];
      const calendar = document.querySelector('.heatmap-calendar');
      const box = (r) => ({
        l: +r.left.toFixed(2),
        t: +r.top.toFixed(2),
        r: +r.right.toFixed(2),
        b: +r.bottom.toFixed(2),
      });
      const name = (el) =>
        el === document.documentElement
          ? 'html'
          : `${el.tagName.toLowerCase()}${
              el.className && typeof el.className === 'string'
                ? '.' + el.className.trim().split(/\s+/).slice(0, 3).join('.')
                : ''
            }`;
      const ov = (el) => ({
        el: name(el),
        ox: getComputedStyle(el).overflowX,
        oy: getComputedStyle(el).overflowY,
        dw: el.scrollWidth - el.clientWidth,
        dh: el.scrollHeight - el.clientHeight,
        barX: el.offsetHeight - el.clientHeight,
        barY: el.offsetWidth - el.clientWidth,
        sl: Math.round(el.scrollLeft),
        st: Math.round(el.scrollTop),
      });
      const scrollers = [];
      for (let el = sq.parentElement; el; el = el.parentElement) {
        const cs = getComputedStyle(el);
        if (
          el === document.documentElement ||
          el.tagName === 'MAIN' ||
          cs.overflowX !== 'visible' ||
          cs.overflowY !== 'visible'
        )
          scrollers.push(el);
      }
      let wrapper = null;
      for (
        let el = sq.parentElement;
        el && el !== calendar.parentElement;
        el = el.parentElement
      ) {
        if (['auto', 'scroll'].includes(getComputedStyle(el).overflowX)) {
          wrapper = el;
          break;
        }
      }
      const container = document.querySelector('.heatmap-container');
      const tip = [...document.querySelectorAll('body *')].find(
        (el) =>
          el.children.length === 0 &&
          rx.test(el.textContent.replace(/\s+/g, ' ')) &&
          getComputedStyle(el).display !== 'none',
      );
      let tooltip = null;
      if (tip) {
        const tr = tip.getBoundingClientRect();
        const clippedBy = [];
        for (let el = tip.parentElement; el; el = el.parentElement) {
          const cs = getComputedStyle(el);
          if (cs.overflowX === 'visible' && cs.overflowY === 'visible')
            continue;
          const er = el.getBoundingClientRect();
          const cl = er.left + el.clientLeft;
          const ct = er.top + el.clientTop;
          if (
            tr.left < cl - 0.5 ||
            tr.top < ct - 0.5 ||
            tr.right > cl + el.clientWidth + 0.5 ||
            tr.bottom > ct + el.clientHeight + 0.5
          )
            clippedBy.push(name(el));
        }
        tooltip = {
          text: tip.textContent.replace(/\s+/g, ' ').trim(),
          box: box(tr),
          clippedBy,
        };
      }
      return {
        viewport: { w: innerWidth, h: innerHeight },
        title: sq.getAttribute('title') || '',
        hovered: squares.findIndex((s) => s.matches(':hover')),
        square: box(sq.getBoundingClientRect()),
        layout: squares.map((s) => [
          s.offsetLeft,
          s.offsetTop,
          s.offsetWidth,
          s.offsetHeight,
        ]),
        rects: squares.map((s) => {
          const r = s.getBoundingClientRect();
          return [r.left, r.top, r.width, r.height].map((v) => +v.toFixed(2));
        }),
        wrapper: wrapper && ov(wrapper),
        wrapperOverflowers:
          wrapper &&
          (() => {
            const wr = wrapper.getBoundingClientRect();
            const right = wr.left + wrapper.clientLeft + wrapper.clientWidth;
            return [...wrapper.querySelectorAll('*')]
              .map((el) => [
                name(el),
                +(
                  el.getBoundingClientRect().right -
                  right +
                  wrapper.scrollLeft
                ).toFixed(2),
              ])
              .filter(([, d]) => d > 0)
              .sort((a, b) => b[1] - a[1])
              .slice(0, 4);
          })(),
        container: container && ov(container),
        container_box: container && box(container.getBoundingClientRect()),
        scrollers: scrollers.map(ov),
        tooltip,
      };
    },
    { sel: SQUARE, idx, re: TOOLTIP_RE.source },
  );

const pickDays = async (page) => {
  const titles = await page
    .locator(SQUARE)
    .evaluateAll((els) => els.map((e) => e.getAttribute('title') || ''));
  const weeks = titles.length / 7;
  const inRange = (i) => titles[i] !== '';
  const col = (w) => [0, 1, 2, 3, 4, 5, 6].map((r) => w * 7 + r);
  const first = col(0).filter(inRange);
  const last = col(weeks - 1).filter(inRange);
  const days = [
    ['first col top', first[0], 'start'],
    ['first col bottom', first.at(-1), 'start'],
    ['2nd col Mon', 7, 'start'],
    ['middle', Math.floor(weeks / 2) * 7 + 3, 'mid'],
    [
      'reported Oct 18',
      titles.findIndex((t) => t.includes(' on Oct 18, ')),
      'mid',
    ],
    ['2nd-last col Sun', (weeks - 2) * 7 + 6, 'end'],
    ['last col top', last[0], 'end'],
    ['last col bottom', last.at(-1), 'end'],
  ].filter(([, i, ,], k, all) => all.findIndex(([, j]) => j === i) === k);
  const padding = [
    ...col(0)
      .filter((i) => !inRange(i))
      .slice(0, 1),
    ...col(weeks - 1)
      .filter((i) => !inRange(i))
      .slice(-1),
  ];
  return { titles, weeks, days, padding };
};

const scrollWrapper = (page, where) =>
  page
    .locator(SQUARE)
    .first()
    .evaluate((sq, where) => {
      let w = sq.parentElement;
      while (w && !['auto', 'scroll'].includes(getComputedStyle(w).overflowX))
        w = w.parentElement;
      if (!w || where === 'mid') return;
      w.scrollLeft = where === 'start' ? 0 : w.scrollWidth;
    }, where);

const restPoint = async (page) => {
  const b = await page.locator('.heatmap-calendar').boundingBox();
  return { x: b.x + 4, y: b.y + 4 };
};

const hoverAndMeasure = async (page, idx, where) => {
  const sq = page.locator(SQUARE).nth(idx);
  await sq.scrollIntoViewIfNeeded();
  await scrollWrapper(page, where);
  const rest = await restPoint(page);
  await page.mouse.move(rest.x, rest.y);
  await settle(page);
  const before = await snap(page, idx);
  const b = await sq.boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  const settled = await settle(page);
  const after = await snap(page, idx);
  await page.mouse.move(rest.x, rest.y);
  await settle(page);
  return { before, after, settled };
};

const overflowDelta = (before, after) =>
  after.scrollers.map((s, i) => ({
    el: s.el,
    dw: s.dw - before.scrollers[i].dw,
    dh: s.dh - before.scrollers[i].dh,
  }));

const report = (label, idx, { before, after, settled }) => {
  const line = {
    label,
    idx,
    settled,
    hovered: after.hovered,
    title: after.title,
    viewport: after.viewport,
    square: after.square,
    wrapper: { before: before.wrapper, after: after.wrapper },
    overflowers: {
      before: before.wrapperOverflowers,
      after: after.wrapperOverflowers,
    },
    container: { before: before.container, after: after.container },
    scrollersBefore: before.scrollers,
    scrollersAfter: after.scrollers,
    tooltip: after.tooltip,
  };
  return line;
};

const expectNoShift = (label, idx, { before, after, settled }) => {
  expect.soft(settled, `${label}: hover settles (no flicker loop)`).toBe(true);
  expect.soft(after.hovered, `${label}: pointer still on the square`).toBe(idx);
  expect
    .soft(after.layout, `${label}: square layout boxes`)
    .toEqual(before.layout);
  const others = (rects) => rects.filter((_, i) => i !== idx);
  expect
    .soft(others(after.rects), `${label}: other squares' rects`)
    .toEqual(others(before.rects));
};

const expectTooltipAbove = (label, after, { inContainer }) => {
  const t = after.tooltip;
  expect.soft(t, `${label}: tooltip shown`).not.toBeNull();
  if (!t) return;
  expect.soft(t.text, `${label}: tooltip text = title`).toBe(after.title);
  expect.soft(t.clippedBy, `${label}: tooltip clipped by`).toEqual([]);
  const v = after.viewport;
  expect
    .soft(
      t.box.l >= 0 && t.box.t >= 0 && t.box.r <= v.w && t.box.b <= v.h,
      `${label}: tooltip ${JSON.stringify(t.box)} inside viewport ${v.w}x${
        v.h
      }`,
    )
    .toBe(true);
  if (inContainer) {
    const c = after.container_box;
    expect
      .soft(
        t.box.l >= c.l && t.box.t >= c.t && t.box.r <= c.r && t.box.b <= c.b,
        `${label}: tooltip ${JSON.stringify(
          t.box,
        )} inside .heatmap-container ${JSON.stringify(c)}`,
      )
      .toBe(true);
  }
  const s = after.square;
  expect
    .soft(
      t.box.b <= s.t + 0.5 && t.box.l < s.r && t.box.r > s.l,
      `${label}: tooltip ${JSON.stringify(t.box)} above square ${JSON.stringify(
        s,
      )}`,
    )
    .toBe(true);
};

for (const scheme of ['light', 'dark']) {
  test(`desktop 1280x900 ${scheme}: hovering edge and middle days adds no scrollbars, tooltip fully visible above the square`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await open(page, { width: 1280, height: 900, scheme });
    const { days } = await pickDays(page);
    for (const [label, idx, where] of days) {
      const m = await hoverAndMeasure(page, idx, where);
      report(`desktop ${scheme} ${label}`, idx, m);
      const tag = `${scheme} ${label}`;
      expect.soft(m.before.wrapper.dw, `${tag}: wrapper x before`).toBe(0);
      expect.soft(m.before.wrapper.dh, `${tag}: wrapper y before`).toBe(0);
      expect.soft(m.after.wrapper.dw, `${tag}: wrapper x overflow`).toBe(0);
      expect.soft(m.after.wrapper.dh, `${tag}: wrapper y overflow`).toBe(0);
      expect.soft(m.after.container.dw, `${tag}: container x overflow`).toBe(0);
      expect.soft(m.after.container.dh, `${tag}: container y overflow`).toBe(0);
      expect
        .soft(
          overflowDelta(m.before, m.after).filter((d) => d.dw || d.dh),
          `${tag}: scroll containers changed by hover (main, html, ...)`,
        )
        .toEqual([]);
      expectNoShift(tag, idx, m);
      expectTooltipAbove(tag, m.after, { inContainer: true });
    }
  });

  test(`narrow 375x800 ${scheme}: wrapper stays horizontally scrollable, edge hovers add no overflow`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await open(page, { width: 375, height: 800, scheme });
    const { days } = await pickDays(page);
    for (const [label, idx, where] of days.filter(([, , w]) => w !== 'mid')) {
      const m = await hoverAndMeasure(page, idx, where);
      report(`narrow ${scheme} ${label}`, idx, m);
      const tag = `narrow ${scheme} ${label}`;
      expect
        .soft(m.before.wrapper.dw, `${tag}: wrapper scrollable`)
        .toBeGreaterThan(0);
      expect
        .soft(m.after.wrapper.dw, `${tag}: wrapper scrollWidth`)
        .toBe(m.before.wrapper.dw);
      expect.soft(m.after.wrapper.dh, `${tag}: wrapper y overflow`).toBe(0);
      expect
        .soft(
          overflowDelta(m.before, m.after).filter((d) => d.dw || d.dh),
          `${tag}: scroll containers changed by hover`,
        )
        .toEqual([]);
      expectNoShift(tag, idx, m);
      expectTooltipAbove(tag, m.after, { inContainer: false });
    }
  });
}

test('padding squares show no tooltip, mouseleave hides it, in-range squares keep a title', async ({
  page,
}) => {
  test.setTimeout(90_000);
  await open(page, { width: 1280, height: 900, scheme: 'light' });
  const { titles, days, padding } = await pickDays(page);
  const year = new Date().getFullYear();
  const daysInYear = (Date.UTC(year + 1, 0, 1) - Date.UTC(year, 0, 1)) / 864e5;
  expect(titles.filter(Boolean)).toHaveLength(daysInYear);
  expect(titles.filter(Boolean).every((t) => TOOLTIP_RE.test(t))).toBe(true);
  expect(padding.length).toBeGreaterThan(0);
  const tip = page.getByText(TOOLTIP_RE);
  for (const idx of padding) {
    const m = await hoverAndMeasure(page, idx, 'mid');
    expect.soft(m.after.title, `padding #${idx} title`).toBe('');
    expect.soft(m.after.tooltip, `padding #${idx} tooltip`).toBeNull();
  }
  const [, idx] = days.find(([l]) => l === 'middle');
  const b = await page.locator(SQUARE).nth(idx).boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await expect(tip).toBeVisible();
  const rest = await restPoint(page);
  await page.mouse.move(rest.x, rest.y);
  await expect(tip).toBeHidden();
});

for (const [vw, vh] of [
  [1280, 900],
  [375, 800],
]) {
  test(`hover scale alone (tooltip hidden) adds no overflow @${vw}x${vh}`, async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await open(page, { width: vw, height: vh, scheme: 'dark' });
    await page.evaluate((re) => {
      const rx = new RegExp(re);
      const hide = () =>
        document.querySelectorAll('body *').forEach((el) => {
          if (
            el.children.length === 0 &&
            rx.test(el.textContent.replace(/\s+/g, ' '))
          )
            el.style.setProperty('display', 'none', 'important');
        });
      new MutationObserver(hide).observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
      });
    }, TOOLTIP_RE.source);
    const { days } = await pickDays(page);
    for (const [label, idx, where] of days.filter(([, , w]) => w !== 'mid')) {
      const m = await hoverAndMeasure(page, idx, where);
      report(`scale-only ${vw} ${label}`, idx, m);
      expect
        .soft(
          overflowDelta(m.before, m.after).filter((d) => d.dw || d.dh),
          `scale-only ${vw} ${label}: scroll containers changed by hover`,
        )
        .toEqual([]);
      expect
        .soft(m.after.wrapper.dh, `scale-only ${vw} ${label}: wrapper y`)
        .toBe(0);
    }
  });
}
