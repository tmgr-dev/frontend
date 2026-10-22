import { expect, test } from '@playwright/test';
import { crc32, deflateSync } from 'node:zlib';
import { mockApp } from './mockApp.mjs';

// TM-10243 QA: AI assistant surface, hostile file contents, size caps, themes and viewports.

const png = (width, height) => {
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const out = Buffer.alloc(body.length + 8);
    out.writeUInt32BE(data.length, 0);
    body.copy(out, 4);
    out.writeUInt32BE(crc32(body), body.length + 4);
    return out;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 2;
  const row = Buffer.concat([
    Buffer.from([0]),
    Buffer.alloc(width * 3, 0).map((_, i) => [70, 130, 220][i % 3]),
  ]);
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(Buffer.concat(Array(height).fill(row)))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
};

const PDF = Buffer.from(
  '%PDF-1.1\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF',
);

const EVIL_MD = [
  '# Heading ok',
  '',
  '<script>window.__mdScript = true</script>',
  '',
  '<img src=x onerror="window.__mdOnerror = true">',
  '',
  '![remote](https://tracker.example.test/pixel.png)',
  '',
  '[bad link](javascript:window.__mdJs=true)',
  '',
  '[good link](https://example.com/page)',
  '',
  '<iframe src="https://evil.example.test/"></iframe>',
  '',
  '<a href="data:text/html,<script>1</script>">data link</a>',
].join('\n');

const HTML_TEXT =
  '<html><body><script>window.__htmlScript = true</script><b id="bold">BOLD</b></body></html>';
const FAKE_PDF_HTML =
  '<html><body><script>parent.__pdfScript = true; window.__pdfScript = true</script>hi</body></html>';

const FILES = [
  { id: 11, name: 'diagram.png', mime_type: 'image/png', size: 2048 },
  { id: 21, name: 'evil.md', mime_type: 'text/markdown', size: 600 },
  { id: 22, name: 'page.html', mime_type: 'text/html', size: 90 },
  { id: 23, name: 'fake.pdf', mime_type: 'application/pdf', size: 90 },
  { id: 24, name: 'big.txt', mime_type: 'text/plain', size: 2 * 1024 * 1024 },
  { id: 25, name: 'report.docx', mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: 5000 },
  { id: 26, name: 'nosize.txt', mime_type: 'text/plain', size: null },
  { id: 27, name: 'real.pdf', mime_type: 'application/pdf', size: 4096 },
];

const setup = async (page, { scheme, files = FILES, ai = false } = {}) => {
  await page.addInitScript(
    ({ scheme }) => {
      sessionStorage.setItem('tmgr:tours:auto', 'true');
      Object.defineProperty(navigator, 'pdfViewerEnabled', { value: true });
      if (scheme) localStorage.setItem('colorScheme', scheme);
    },
    { scheme },
  );
  await mockApp(page);
  const comment = {
    id: 1,
    message: 'See the attached files',
    user: { id: 1, name: 'Test User' },
    created_at: '2026-09-13T10:00:00Z',
    updated_at: '2026-09-13T10:00:00Z',
    reactions: [],
    files,
  };
  await page.route('**/api/tasks/1/comments**', (route) =>
    route.fulfill({ json: { data: [comment] } }),
  );
  await page.route('**/api/files/*/signed-url', (route) =>
    route.fulfill({ status: 503, json: {} }),
  );
  const requests = [];
  await page.route('**/api/files/*/content*', (route) => {
    const id = Number(new URL(route.request().url()).pathname.split('/')[3]);
    requests.push({ id, auth: route.request().headers().authorization });
    if (id === 11)
      return route.fulfill({ contentType: 'image/png', body: png(400, 300) });
    if (id === 21)
      return route.fulfill({ contentType: 'text/markdown', body: EVIL_MD });
    if (id === 22)
      return route.fulfill({ contentType: 'text/html', body: HTML_TEXT });
    if (id === 23)
      return route.fulfill({ contentType: 'text/html', body: FAKE_PDF_HTML });
    if (id === 24)
      return route.fulfill({
        contentType: 'text/plain',
        body: 'x'.repeat(2 * 1024 * 1024),
      });
    if (id === 25)
      return route.fulfill({
        contentType: 'application/octet-stream',
        body: 'PK',
      });
    if (id === 26)
      return route.fulfill({
        contentType: 'text/plain',
        body: 'y'.repeat(1024 * 1024 + 10),
      });
    if (id === 27) return route.fulfill({ contentType: 'application/pdf', body: PDF });
    return route.fulfill({ status: 404, json: {} });
  });
  if (ai) {
    const history = [
      { id: 1, role: 'user', content: 'look at these', status: 'done', steps: [], created_at: '2026-09-13T10:00:00Z', files },
      { id: 2, role: 'assistant', content: 'ok', status: 'done', steps: [], created_at: '2026-09-13T10:00:01Z' },
    ];
    await page.route('**/api/agent/conversations**', async (route) => {
      const url = route.request().url();
      if (url.endsWith('/messages') && route.request().method() === 'GET')
        return route.fulfill({ json: { data: history } });
      return route.fulfill({
        json: { data: { id: 7, workspace_id: 1, task_id: null, created_at: 'x' } },
      });
    });
  }
  return requests;
};

const viewerOf = (page) =>
  page.getByRole('dialog').filter({ has: page.getByTestId('viewer-header') });

const openModal = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  await page.locator('[data-comment-id="1"]').scrollIntoViewIfNeeded();
};

const openFile = async (page, name) => {
  const root = page.locator('[data-comment-id="1"]');
  await root.getByRole('button', { name: new RegExp(name.replace('.', '\\.')) }).click();
  return viewerOf(page);
};

test.describe('hostile content stays inert', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test('markdown with script, onerror, remote img, js link, iframe, data link', async ({ page }) => {
    const requests = await setup(page);
    const remote = [];
    page.on('request', (r) => {
      if (r.url().includes('example.test') || r.url().includes('example.com')) remote.push(r.url());
    });
    await openModal(page);
    const viewer = await openFile(page, 'evil.md');
    const md = viewer.getByTestId('viewer-markdown');
    await expect(md.getByRole('heading', { name: 'Heading ok' })).toBeVisible();
    await page.waitForTimeout(500);
    await expect(md.locator('script')).toHaveCount(0);
    await expect(md.locator('iframe')).toHaveCount(0);
    const imgSrcs = await md.locator('img').evaluateAll((is) => is.map((i) => i.getAttribute('src')));
    for (const src of imgSrcs) expect(src).not.toMatch(/^(https?:)?\/\//);
    expect(await md.locator('[onerror]').count()).toBe(0);
    const hrefs = await md.locator('a').evaluateAll((as) => as.map((a) => a.getAttribute('href')));
    for (const h of hrefs) expect(h || '').not.toMatch(/^(javascript|data):/i);
    expect(
      await page.evaluate(() => [window.__mdScript, window.__mdOnerror, window.__mdJs]),
    ).toEqual([undefined, undefined, undefined]);
    expect(remote.filter((u) => u.includes('tracker.example.test'))).toEqual([]);
    expect(requests.every((r) => r.id !== 21 || !!r.auth)).toBe(true);
  });

  test('clicking a markdown link does not navigate the app or run js', async ({ page, context }) => {
    await setup(page);
    await openModal(page);
    const viewer = await openFile(page, 'evil.md');
    const md = viewer.getByTestId('viewer-markdown');
    await expect(md.getByRole('heading', { name: 'Heading ok' })).toBeVisible();
    const link = md.getByRole('link', { name: 'good link' });
    const attrs = await link.evaluate((a) => ({ target: a.target, rel: a.rel, href: a.href }));
    expect(attrs.rel).toContain('noopener');
  });

  test('raw html media in markdown makes no outbound request', async ({ page }) => {
    const raw = [
      '<img src="https://tracker.example.test/a.png">',
      '<img src="//tracker.example.test/b.png">',
      '<img srcset="https://tracker.example.test/c.png 1x" src="">',
      '<video src="https://tracker.example.test/v.mp4" poster="https://tracker.example.test/p.png"></video>',
      '<audio src="https://tracker.example.test/a.mp3"></audio>',
      '<svg onload="window.__svg=1"><image href="https://tracker.example.test/s.png"/></svg>',
      '<style>@import url(https://tracker.example.test/x.css); body{background:url(https://tracker.example.test/bg.png)}</style>',
      '<link rel="stylesheet" href="https://tracker.example.test/l.css">',
      '<div style="background:url(https://tracker.example.test/d.png)">styled</div>',
      '<form action="https://tracker.example.test/f"><input></form>',
      '<object data="https://tracker.example.test/o"></object><embed src="https://tracker.example.test/e">',
      '<meta http-equiv="refresh" content="0;url=https://tracker.example.test/m"><base href="https://tracker.example.test/">',
      '<a href="https://example.com/z" target="_self">z</a>',
    ].join('\n\n');
    await setup(page);
    await page.route('**/api/files/21/content*', (route) =>
      route.fulfill({ contentType: 'text/markdown', body: raw }),
    );
    const remote = [];
    page.on('request', (r) => {
      if (r.url().includes('tracker.example.test')) remote.push(r.url());
    });
    await openModal(page);
    const viewer = await openFile(page, 'evil.md');
    const md = viewer.getByTestId('viewer-markdown');
    await expect(md).toBeVisible();
    await page.waitForTimeout(800);
    expect(remote).toEqual([]);
    expect(await page.evaluate(() => window.__svg)).toBeUndefined();
    expect(await page.evaluate(() => document.querySelector('base')?.href)).toBeUndefined();
  });

  test('html file is shown as escaped text and never executes', async ({ page }) => {
    await setup(page);
    await openModal(page);
    const viewer = await openFile(page, 'page.html');
    const pre = viewer.getByTestId('viewer-text');
    await expect(pre).toContainText('<script>window.__htmlScript = true</script>');
    await expect(pre.locator('script, b')).toHaveCount(0);
    expect(await page.evaluate(() => window.__htmlScript)).toBeUndefined();
    await expect(viewer.locator('iframe')).toHaveCount(0);
  });

  test('pdf-mime file with html bytes: iframe is blob pdf, no script in app', async ({ page }) => {
    await setup(page);
    await openModal(page);
    const viewer = await openFile(page, 'fake.pdf');
    const frame = viewer.getByTestId('viewer-pdf');
    await expect(frame).toBeVisible();
    const src = await frame.getAttribute('src');
    expect(src).toMatch(/^blob:/);
    const blobType = await page.evaluate(async (s) => (await fetch(s)).blob().then((b) => b.type), src);
    expect(blobType).toBe('application/pdf');
    await page.waitForTimeout(800);
    expect(await page.evaluate(() => [window.__pdfScript])).toEqual([undefined]);
    const allSrc = await page.evaluate(() =>
      [...document.querySelectorAll('iframe,img,embed,object')].map((e) => e.src || e.data),
    );
    for (const s of allSrc.filter(Boolean)) expect(s).not.toMatch(/token|signed|api\/files/i);
  });

  // Known limit (D-009/D-005): key events inside the PDF iframe never reach the app, so Esc
  // does not close the viewer while the iframe has focus. Remove test.fail when that is fixed.
  test('pdf iframe: Esc with focus inside closes the viewer', async ({ page }) => {
    test.fail(true, 'Esc is consumed by the PDF iframe');
    await setup(page);
    await openModal(page);
    const viewer = await openFile(page, 'real.pdf');
    const frame = viewer.getByTestId('viewer-pdf');
    await expect(frame).toBeVisible();
    await page.waitForTimeout(1500);
    await frame.click({ position: { x: 100, y: 100 } });
    await page.keyboard.press('Escape');
    await expect(viewer).toBeHidden({ timeout: 2000 });
  });

  test('size caps and unsupported types go to a download panel', async ({ page }) => {
    const requests = await setup(page);
    await openModal(page);
    let viewer = await openFile(page, 'big.txt');
    await expect(viewer.getByTestId('viewer-unsupported')).toBeVisible();
    await page.waitForTimeout(300);
    expect(requests.filter((r) => r.id === 24)).toHaveLength(0);
    await page.keyboard.press('ArrowRight');
    await expect(viewer.getByTestId('viewer-unsupported')).toBeVisible();
    // size unknown: treated as over the cap, so nothing is fetched
    await page.keyboard.press('ArrowRight');
    await expect(viewer.getByTestId('viewer-unsupported')).toBeVisible();
    await page.waitForTimeout(300);
    expect(requests.filter((r) => r.id === 26)).toHaveLength(0);
  });

  test('docx -> download panel; download works with bearer content', async ({ page }) => {
    const requests = await setup(page);
    await openModal(page);
    const viewer = await openFile(page, 'report.docx');
    await expect(viewer.getByTestId('viewer-unsupported')).toBeVisible();
    const [download] = await Promise.all([
      page.waitForEvent('download'),
      viewer.getByTestId('viewer-unsupported').getByRole('button', { name: 'Download' }).click(),
    ]);
    expect(download.suggestedFilename()).toBe('report.docx');
    expect(requests.some((r) => r.id === 25 && r.auth)).toBe(true);
  });
});

test.describe('AI assistant surface', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test('viewer opens from an AI message; Esc closes only the viewer; focus returns', async ({ page }) => {
    await setup(page, { ai: true });
    await page.goto('/demo/list');
    await page.getByRole('button', { name: 'Ask AI', exact: true }).click();
    const panel = page.locator('[aria-label="AI assistant"]');
    await expect(panel).toBeVisible();
    const thumb = panel.locator('img[alt="diagram.png"]');
    await expect(thumb).toBeVisible();
    const box = await thumb.locator('xpath=..').boundingBox();
    expect([Math.round(box.width), Math.round(box.height)]).toEqual([96, 96]);
    await expect(panel.getByText('evil.md')).toBeVisible();

    const opener = panel.getByRole('button', { name: /diagram\.png/ });
    await opener.click();
    const viewer = viewerOf(page);
    await expect(viewer).toBeVisible();
    await expect.poll(async () => (await viewer.boundingBox()).width).toBe(1440);
    await expect(viewer.getByText('1 / 8')).toBeVisible();
    await page.keyboard.press('ArrowRight');
    await expect(viewer.getByTestId('viewer-markdown')).toBeVisible();
    await page.keyboard.press('ArrowLeft');
    await expect(viewer.locator('img[alt="diagram.png"]').last()).toBeVisible();

    await page.keyboard.press('Escape');
    await expect(viewer).toBeHidden();
    await expect(panel).toBeVisible();
    await expect(opener).toBeFocused();

    // a second Esc is allowed to close the panel (after debounce)
    await page.waitForTimeout(1100);
    await page.keyboard.press('Escape');
    await expect(panel).toBeHidden();
  });
});

for (const scheme of ['light', 'dark']) {
  for (const vp of [
    { width: 1440, height: 900 },
    { width: 390, height: 844 },
  ]) {
    test(`screenshots ${scheme} ${vp.width}`, async ({ page }, testInfo) => {
      await page.setViewportSize(vp);
      await setup(page, { scheme });
      await openModal(page);
      const comment = page.locator('[data-comment-id="1"]');
      await expect(comment.locator('img[alt="diagram.png"]')).toBeVisible();
      await comment.screenshot({ path: testInfo.outputPath(`comment-${scheme}-${vp.width}.png`) });
      const before = await comment.boundingBox();
      let viewer = await openFile(page, 'diagram.png');
      await expect(viewer.locator('img[alt="diagram.png"]').last()).toBeVisible();
      await page.waitForTimeout(700);
      await page.screenshot({ path: testInfo.outputPath(`viewer-img-${scheme}-${vp.width}.png`) });
      await page.keyboard.press('ArrowRight');
      await expect(viewer.getByTestId('viewer-markdown')).toBeVisible();
      await page.waitForTimeout(700);
      await page.screenshot({ path: testInfo.outputPath(`viewer-md-${scheme}-${vp.width}.png`) });
      for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowRight');
      await expect(viewer.getByTestId('viewer-unsupported')).toBeVisible();
      await page.waitForTimeout(700);
      await page.screenshot({ path: testInfo.outputPath(`viewer-unsupported-${scheme}-${vp.width}.png`) });
      await page.keyboard.press('Escape');
      await expect(viewer).toBeHidden();
      const after = await comment.boundingBox();
      expect(after.height).toBe(before.height);
    });
  }
}

test.describe('regression: existing galleries', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  const mockFiles = async (page, list) => {
    await page.route('**/api/files/*/signed-url', (route) => {
      const id = Number(new URL(route.request().url()).pathname.split('/')[3]);
      return route.fulfill({
        json: {
          data: {
            url: `/api/files/${id}/content?signed=fixture`,
            expires_at: '2099-01-01T00:00:00Z',
          },
        },
      });
    });
    await page.route('**/api/files/*/content*', (route) =>
      route.fulfill({ contentType: 'image/png', body: png(300, 200) }),
    );
    await page.route('**/api/tasks/1/files', (route) =>
      route.fulfill({ json: { data: list } }),
    );
    await page.route('**/api/workspaces/1/files*', (route) =>
      route.fulfill({
        json: {
          data: list.map((f) => ({ ...f, task: { id: 1, title: 'T', key: 'T1' } })),
          meta: { total: list.length, last_page: 1, current_page: 1 },
        },
      }),
    );
  };
  const list = [
    { id: 31, name: 'one.png', mime_type: 'image/png', size: 100 },
    { id: 32, name: 'doc.pdf', mime_type: 'application/pdf', size: 100 },
    { id: 33, name: 'two.png', mime_type: 'image/png', size: 100 },
  ];

  test('workspace files page opens and navigates images, only images in the gallery', async ({ page }) => {
    await page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));
    await mockApp(page);
    await mockFiles(page, list);
    await page.goto('/demo/files');
    const open = page.locator('button[title="Open one.png"]');
    await expect(open).toBeVisible();
    await open.click();
    const viewer = viewerOf(page);
    await expect(viewer).toBeVisible();
    await expect(viewer.getByText('1 / 2')).toBeVisible();
    await expect(viewer.locator('img[alt="one.png"]').last()).toBeVisible();
    await expect(viewer.getByRole('button', { name: 'Zoom in' })).toBeVisible();
    await page.keyboard.press('ArrowRight');
    await expect(viewer.getByText('2 / 2')).toBeVisible();
    await expect(viewer.locator('img[alt="two.png"]').last()).toBeVisible();
    await viewer.getByRole('button', { name: 'Zoom in' }).click();
    await page.keyboard.press('Escape');
    await expect(viewer).toBeHidden();
    await expect(open).toBeFocused();
  });

  test('task attachments gallery opens inside the task modal; Esc keeps the modal', async ({ page }) => {
    await page.addInitScript(() => sessionStorage.setItem('tmgr:tours:auto', 'true'));
    await mockApp(page);
    await mockFiles(page, list);
    await page.goto('/demo/list');
    await page.locator('[data-task-id="1"]').first().click();
    const section = page.locator('.task-attachments');
    await section.scrollIntoViewIfNeeded();
    const open = section.locator('button[title="Open one.png"]');
    await expect(open).toBeVisible();
    await open.click();
    const viewer = viewerOf(page);
    await expect(viewer).toBeVisible();
    await expect(viewer.getByText('1 / 2')).toBeVisible();
    await page.keyboard.press('ArrowLeft');
    await expect(viewer.getByText('2 / 2')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(viewer).toBeHidden();
    await expect(section).toBeVisible();
    await expect(open).toBeFocused();
  });
});
