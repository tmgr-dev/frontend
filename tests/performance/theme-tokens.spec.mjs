import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

// TM-697 leftovers: token opacity modifiers generate CSS, text on brand fills follows the theme's
// on-accent colour, the app root uses the token foreground, and a wide Modal paints above the sidebar.

const bootstrap = async (page, url, viewport, mockOpts = {}) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await page.setViewportSize(viewport);
  await mockApp(page, mockOpts);
  await page.goto(url);
};

const applyTheme = async (page, id, scheme) => {
  await page.evaluate(
    async ([themeId, colorScheme]) => {
      const m = await import('/src/theme/applyTheme.ts');
      m.applyThemeToDocument(themeId, colorScheme);
    },
    [id, scheme],
  );
};

// Computed colours come back as rgb()/rgba() or color(srgb r g b / a) depending on how they were built.
const parseColor = (value) => {
  const nums = value.match(/-?\d*\.?\d+(?:e-?\d+)?/g).map(Number);
  if (value.startsWith('color(srgb')) {
    const [r, g, b, a = 1] = nums;
    return { rgb: [r, g, b].map((v) => Math.round(v * 255)), a };
  }
  const [r, g, b, a = 1] = nums;
  return { rgb: [r, g, b], a };
};

const probe = (page, className, prop) =>
  page.evaluate(
    ([cls, p]) => {
      const el = document.createElement('div');
      el.className = cls;
      el.style.cssText = 'width:40px;height:40px;';
      document.body.appendChild(el);
      const value = getComputedStyle(el)[p];
      el.remove();
      return value;
    },
    [className, prop],
  );

test.describe('token opacity modifiers', () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test('modified token classes are translucent, unmodified ones opaque', async ({
    page,
  }) => {
    await bootstrap(page, '/demo/list', { width: 1280, height: 800 });
    await page.getByPlaceholder('Search').first().waitFor();
    for (const [cls, prop] of [
      ['bg-brand/10', 'backgroundColor'],
      ['bg-brand/60', 'backgroundColor'],
      ['bg-status-fix-bg/40', 'backgroundColor'],
      ['border border-brand/40', 'borderTopColor'],
      ['text-ink-subtle/70', 'color'],
    ]) {
      const { a } = parseColor(await probe(page, cls, prop));
      expect(a, `${cls} alpha`).toBeGreaterThan(0);
      expect(a, `${cls} alpha`).toBeLessThan(1);
    }
    const ring = await probe(page, 'ring-2 ring-brand/40', 'boxShadow');
    expect(ring).not.toBe('none');
    expect(ring).toMatch(/(\/ |, )0?\.4\)/);
    const { a: solid } = parseColor(
      await probe(page, 'bg-brand', 'backgroundColor'),
    );
    expect(solid).toBe(1);
  });

  test('an unmodified token class resolves to the same colour as the bare variable', async ({
    page,
  }) => {
    await bootstrap(page, '/demo/list', { width: 1280, height: 800 });
    await page.getByPlaceholder('Search').first().waitFor();
    for (const [id, scheme] of [
      ['default', 'default'],
      ['default', 'dark'],
      ['dracula', 'dark'],
    ]) {
      await applyTheme(page, id, scheme);
      for (const [cls, variable] of [
        ['bg-brand', '--brand-color'],
        ['bg-surface', '--bg-raised'],
        ['bg-status-fix-bg', '--s-fix-bg'],
        ['bg-brand-bg', '--brand-bg-color'],
      ]) {
        const [fromClass, fromVar] = await page.evaluate(
          async ([c, v]) => {
            const mk = (style, cls) => {
              const el = document.createElement('div');
              el.className = cls;
              el.style.cssText = `position:fixed;left:0;top:0;width:10px;height:10px;${style}`;
              document.body.appendChild(el);
              return el;
            };
            const a = mk('', c);
            const b = mk(`background-color:var(${v});`, '');
            const toRgb = (el) => {
              const probeEl = document.createElement('canvas');
              probeEl.width = probeEl.height = 1;
              const ctx = probeEl.getContext('2d');
              ctx.fillStyle = '#fff';
              ctx.fillRect(0, 0, 1, 1);
              ctx.fillStyle = getComputedStyle(el).backgroundColor;
              ctx.fillRect(0, 0, 1, 1);
              return Array.from(ctx.getImageData(0, 0, 1, 1).data);
            };
            const out = [toRgb(a), toRgb(b)];
            a.remove();
            b.remove();
            return out;
          },
          [cls, variable],
        );
        for (let i = 0; i < 3; i++) {
          expect(
            Math.abs(fromClass[i] - fromVar[i]),
            `${id}/${scheme} ${cls} channel ${i}`,
          ).toBeLessThanOrEqual(1);
        }
      }
    }
  });
});

test.describe('on-accent text on brand fills', () => {
  test.setTimeout(90000);

  // The button animates its colour (transition-all), so wait for the resting value.
  const expectTimerColor = async (page, id, scheme, rgb) => {
    await applyTheme(page, id, scheme);
    const btn = page.locator('button[title="Start timer"]');
    await expect(btn).toBeVisible();
    await expect
      .poll(
        async () => {
          const value = await btn.evaluate((el) => getComputedStyle(el).color);
          if (!value.startsWith('color(srgb') && !value.startsWith('rgb'))
            return null;
          const { rgb: got, a } = parseColor(value);
          return [...got, a];
        },
        { message: `${id}/${scheme} on-brand text` },
      )
      .toEqual([...rgb, 1]);
  };

  const openTask = async (page) => {
    await bootstrap(page, '/demo/list', { width: 1440, height: 900 });
    await page.locator('[data-task-id="1"]').first().click({ timeout: 60000 });
    await page.getByPlaceholder('Task name').waitFor();
  };

  test('white in default light and dark, theme on-accent in cobalt2', async ({
    page,
  }) => {
    await openTask(page);
    await expectTimerColor(page, 'default', 'default', [255, 255, 255]);
    await expectTimerColor(page, 'default', 'dark', [255, 255, 255]);
    await expectTimerColor(page, 'cobalt2', 'dark', [0x19, 0x35, 0x49]);
    await expectTimerColor(page, 'gruvbox-dark', 'dark', [0x28, 0x28, 0x28]);
  });

  test('an unknown stored theme keeps the default white', async ({ page }) => {
    await openTask(page);
    await expectTimerColor(page, 'no-such-theme', 'default', [255, 255, 255]);
    await expect(page.locator('html')).not.toHaveClass(/theme-/);
  });
});

test.describe('app root foreground', () => {
  test('root colour is the token foreground in every theme', async ({
    page,
  }) => {
    await bootstrap(page, '/demo/list', { width: 1280, height: 800 });
    await page.getByPlaceholder('Search').first().waitFor();
    for (const [id, scheme] of [
      ['default', 'default'],
      ['default', 'dark'],
      ['dracula', 'dark'],
      ['github-light', 'default'],
    ]) {
      await applyTheme(page, id, scheme);
      // body animates its colour (transition-colors), root inherits the animated value.
      await expect
        .poll(
          async () => {
            const v = await page.evaluate(() => {
              const rootEl = document.querySelector('#app div.font-sans');
              const ref = document.createElement('i');
              ref.style.color = 'var(--fg)';
              document.body.appendChild(ref);
              const fg = getComputedStyle(ref).color;
              ref.remove();
              return {
                fg,
                root: getComputedStyle(rootEl).color,
                body: getComputedStyle(document.body).color,
              };
            });
            const rgb = (c) => JSON.stringify(parseColor(c).rgb);
            return rgb(v.root) === rgb(v.fg) && rgb(v.body) === rgb(v.fg)
              ? 'same as --fg'
              : JSON.stringify(v);
          },
          { message: `${id}/${scheme} root and body colour` },
        )
        .toBe('same as --fg');
    }
  });
});

test.describe('workspace member avatars', () => {
  for (const [id, scheme] of [
    ['dracula', 'dark'],
    ['github-light', 'default'],
    ['default', 'default'],
    ['default', 'dark'],
  ]) {
    test(`initials are readable in ${id}/${scheme}`, async ({ page }) => {
      await bootstrap(page, '/demo/board', { width: 1280, height: 700 });
      await page
        .locator('[class*="-space-x-2"] button.rounded-full')
        .first()
        .waitFor();
      await applyTheme(page, id, scheme);
      await expect
        .poll(
          () =>
            page.evaluate(() => {
              const resolve = (css) => {
                const cv = document.createElement('canvas');
                cv.width = cv.height = 1;
                const ctx = cv.getContext('2d');
                ctx.fillStyle = css;
                ctx.fillRect(0, 0, 1, 1);
                return Array.from(ctx.getImageData(0, 0, 1, 1).data);
              };
              const lum = ([r, g, b]) =>
                [r, g, b]
                  .map((v) => v / 255)
                  .map((c) =>
                    c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4,
                  )
                  .reduce(
                    (acc, c, i) => acc + c * [0.2126, 0.7152, 0.0722][i],
                    0,
                  );
              const button = document.querySelector(
                '[class*="-space-x-2"] button.rounded-full',
              );
              const initials = button.querySelector('span');
              const fg = lum(resolve(getComputedStyle(initials).color));
              const bg = lum(resolve(getComputedStyle(button).backgroundColor));
              const [hi, lo] = fg > bg ? [fg, bg] : [bg, fg];
              return Math.round(((hi + 0.05) / (lo + 0.05)) * 100) / 100;
            }),
          { message: `${id}/${scheme} initials contrast` },
        )
        .toBeGreaterThanOrEqual(4.5);
    });
  }
});

test.describe('Board dialog next to the sidebar', () => {
  test.setTimeout(90000);

  for (const width of [1280, 1024]) {
    test(`800px dialog is not covered by the sidebar at ${width}px`, async ({
      page,
    }) => {
      await bootstrap(
        page,
        '/demo/board',
        { width, height: 700 },
        {
          multiStatus: true,
        },
      );
      const add = page.locator('[data-tour="board.add-in-column"]').first();
      await add.waitFor({ state: 'attached' });
      await add
        .locator('xpath=ancestor::div[contains(@class,"ml-auto")][1]')
        .locator('button')
        .last()
        .click();
      await page.getByText('Edit status').click();
      const overlay = page.locator('[data-name="overlay"]');
      await expect(overlay).toHaveCount(1);
      const panel = overlay.locator('> div').nth(1);
      await panel.evaluate((el) => {
        el.style.width = '800px';
      });
      const m = await page.evaluate(() => {
        const o = document.querySelector('[data-name="overlay"]');
        const p = o.children[1].getBoundingClientRect();
        const s = document
          .querySelector('[data-sidebar="sidebar"]')
          .getBoundingClientRect();
        const covered = [10, p.width / 2].some((dx) => {
          const el = document.elementFromPoint(
            Math.max(p.left + dx, 1),
            p.top + 40,
          );
          return !o.children[1].contains(el);
        });
        return {
          left: p.left,
          right: p.right,
          sidebarRight: s.right,
          vw: window.innerWidth,
          covered,
        };
      });
      expect(m.left).toBeGreaterThanOrEqual(0);
      expect(m.right).toBeLessThanOrEqual(m.vw);
      expect(m.left >= m.sidebarRight || !m.covered).toBe(true);
    });
  }
});
