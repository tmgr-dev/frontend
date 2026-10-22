import { expect, test } from '@playwright/test';
import { crc32, deflateSync } from 'node:zlib';
import { mockApp } from './mockApp.mjs';

// TM-10243: files in a comment get thumbnails/cards and open in a full-window viewer.

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

const MARKDOWN = [
  '# Release notes',
  '',
  'Visible **bold** text.',
  '',
  '<script>window.__markdownScript = true</script>',
  '',
  '![tracker](https://tracker.example.test/pixel.png)',
].join('\n');

const files = [
  { id: 11, name: 'diagram.png', mime_type: 'image/png', size: 2048 },
  { id: 12, name: 'spec.pdf', mime_type: 'application/pdf', size: 4096 },
  { id: 13, name: 'notes.md', mime_type: 'text/markdown', size: 300 },
];

const setup = async (
  page,
  {
    contentStatus = {},
    pdfViewer = true,
    imageSize = [400, 300],
    attached = files,
  } = {},
) => {
  await page.addInitScript((pdfViewerEnabled) => {
    sessionStorage.setItem('tmgr:tours:auto', 'true');
    Object.defineProperty(navigator, 'pdfViewerEnabled', {
      value: pdfViewerEnabled,
    });
  }, pdfViewer);
  await mockApp(page);
  const comment = {
    id: 1,
    message: 'See the attached files',
    user: { id: 1, name: 'Test User' },
    created_at: '2026-09-13T10:00:00Z',
    updated_at: '2026-09-13T10:00:00Z',
    reactions: [],
    files: attached,
  };
  await page.route('**/api/tasks/1/comments**', (route) =>
    route.fulfill({ json: { data: [comment] } }),
  );
  await page.route('**/api/files/*/signed-url', (route) =>
    route.fulfill({ status: 503, json: {} }),
  );
  const requested = [];
  await page.route('**/api/files/*/content*', (route) => {
    const id = Number(new URL(route.request().url()).pathname.split('/')[3]);
    requested.push(id);
    if (contentStatus[id]) {
      return route.fulfill({ status: contentStatus[id], json: {} });
    }
    if (id === 11) {
      return route.fulfill({
        contentType: 'image/png',
        body: png(...imageSize),
      });
    }
    if (id === 12) {
      return route.fulfill({ contentType: 'text/html', body: PDF });
    }
    return route.fulfill({ contentType: 'text/markdown', body: MARKDOWN });
  });
  return requested;
};

const viewerOf = (page) =>
  page.getByRole('dialog').filter({ has: page.getByTestId('viewer-header') });

const openModal = async (page) => {
  await page.goto('/demo/list');
  await page.locator('[data-task-id="1"]').first().click();
  await page.locator('[data-comment-id="1"]').scrollIntoViewIfNeeded();
};

test.describe('chat attachment viewer', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test('thumbnail and cards open a viewer with zoom, pdf and sanitized markdown', async ({
    page,
  }) => {
    await setup(page);
    await openModal(page);
    const comment = page.locator('[data-comment-id="1"]');

    const thumb = comment.locator('img[alt="diagram.png"]');
    await expect(thumb).toBeVisible();
    const box = await thumb.locator('xpath=..').boundingBox();
    expect(Math.round(box.width)).toBe(96);
    expect(Math.round(box.height)).toBe(96);
    await expect(comment.getByText('spec.pdf')).toBeVisible();
    await expect(comment.getByText('notes.md')).toBeVisible();

    const opener = comment.getByRole('button', { name: /diagram\.png/ });
    await opener.click();
    const viewer = viewerOf(page);
    await expect(viewer).toBeVisible();
    await expect
      .poll(async () => {
        const { width, height } = await viewer.boundingBox();
        return [width, height];
      })
      .toEqual([1440, 900]);
    await expect(viewer.getByText('1 / 3')).toBeVisible();

    const transform = viewer.getByTestId('viewer-image-transform');
    await expect(viewer.locator('img[alt="diagram.png"]').last()).toBeVisible();
    const before = await transform.evaluate((el) => el.style.transform);
    await viewer.getByRole('button', { name: 'Zoom in' }).click();
    const zoomed = await transform.evaluate((el) => el.style.transform);
    expect(zoomed).not.toBe(before);
    await page.keyboard.press('-');
    expect(await transform.evaluate((el) => el.style.transform)).toBe(before);
    await page.keyboard.press('+');
    await page.keyboard.press('0');
    expect(await transform.evaluate((el) => el.style.transform)).toBe(before);

    await page.keyboard.press('ArrowRight');
    const frame = viewer.getByTestId('viewer-pdf');
    await expect(frame).toBeVisible();
    expect(await frame.getAttribute('src')).toMatch(/^blob:/);

    await page.keyboard.press('ArrowRight');
    const markdown = viewer.getByTestId('viewer-markdown');
    await expect(
      markdown.getByRole('heading', { name: 'Release notes' }),
    ).toBeVisible();
    await expect(markdown.locator('script')).toHaveCount(0);
    await expect(markdown.locator('img')).toHaveCount(0);
    expect(await page.evaluate(() => window.__markdownScript)).toBeUndefined();

    await page.keyboard.press('Escape');
    await expect(viewer).toBeHidden();
    await expect(page.locator('[data-comment-id="1"]')).toBeVisible();
    await expect(opener).toBeFocused();
  });

  test('wheel, ctrl+wheel and 100% zoom the image and a drag pans it', async ({
    page,
  }) => {
    await setup(page, { imageSize: [1800, 1200] });
    await openModal(page);
    await page
      .locator('[data-comment-id="1"]')
      .getByRole('button', { name: /diagram\.png/ })
      .click();
    const viewer = viewerOf(page);
    const transform = viewer.getByTestId('viewer-image-transform');
    const read = () => transform.evaluate((el) => el.style.transform);
    await expect(viewer.locator('img[alt="diagram.png"]').last()).toBeVisible();
    const fit = await read();

    await page.mouse.move(720, 450);
    await page.mouse.wheel(0, -300);
    await expect.poll(read).not.toBe(fit);
    const wheeled = await read();
    await page.keyboard.down('Control');
    await page.mouse.wheel(0, -50);
    await page.keyboard.up('Control');
    await expect.poll(read).not.toBe(wheeled);

    await viewer.getByRole('button', { name: 'Fit to window' }).click();
    expect(await read()).toBe(fit);
    for (let i = 0; i < 8; i++) await page.keyboard.press('+');
    await expect(viewer.getByRole('button', { name: 'Zoom in' })).toBeEnabled();
    const zoomedIn = await read();
    await page.mouse.move(720, 450);
    await page.mouse.down();
    await page.mouse.move(820, 500, { steps: 4 });
    await page.mouse.up();
    expect(await read()).toMatch(/translate\(100px, 50px\)/);
    expect(await read()).not.toBe(zoomedIn);

    await viewer.getByRole('button', { name: 'Fit to window' }).click();
    await viewer.getByRole('button', { name: 'Actual size' }).click();
    expect(await read()).toMatch(/scale\(1\.[1-9]/);
  });

  test('pdf falls back to a download panel when the browser has no pdf viewer', async ({
    page,
  }) => {
    await setup(page, { pdfViewer: false });
    await openModal(page);
    await page
      .locator('[data-comment-id="1"]')
      .getByRole('button', { name: /spec\.pdf/ })
      .click();
    const viewer = viewerOf(page);
    await expect(
      viewer.getByText('Preview is not available here'),
    ).toBeVisible();
    await expect(viewer.locator('iframe')).toHaveCount(0);
    await expect(
      viewer.getByRole('button', { name: 'Download' }).first(),
    ).toBeVisible();
  });

  test('a failed content request shows an error with retry and keeps download', async ({
    page,
  }) => {
    const requested = await setup(page, { contentStatus: { 13: 500 } });
    await openModal(page);
    await page
      .locator('[data-comment-id="1"]')
      .getByRole('button', { name: /notes\.md/ })
      .click();
    const viewer = viewerOf(page);
    await expect(viewer.getByTestId('viewer-error')).toContainText(
      'Could not load file',
    );
    await expect(
      viewer.getByRole('button', { name: 'Download' }),
    ).toBeVisible();
    const attempts = requested.filter((id) => id === 13).length;
    await viewer.getByRole('button', { name: 'Retry' }).click();
    await expect
      .poll(() => requested.filter((id) => id === 13).length)
      .toBe(attempts + 1);
  });

  test('a markdown file with no known size is not fetched and offers Download', async ({
    page,
  }) => {
    const requested = await setup(page, {
      attached: files.map((file) =>
        file.id === 13 ? { ...file, size: null } : file,
      ),
    });
    await openModal(page);
    await page
      .locator('[data-comment-id="1"]')
      .getByRole('button', { name: /notes\.md/ })
      .click();
    const viewer = viewerOf(page);
    await expect(viewer.getByTestId('viewer-unsupported')).toBeVisible();
    expect(requested).not.toContain(13);
  });

  test('a failed download shows a message and the button works again', async ({
    page,
  }) => {
    const contentStatus = {};
    await setup(page, { contentStatus });
    await openModal(page);
    await page
      .locator('[data-comment-id="1"]')
      .getByRole('button', { name: /diagram\.png/ })
      .click();
    const viewer = viewerOf(page);
    await expect(viewer.locator('img[alt="diagram.png"]').last()).toBeVisible();
    contentStatus[11] = 500;
    const download = viewer.getByRole('button', { name: 'Download' });
    await download.click();
    await expect(viewer.getByTestId('viewer-download-error')).toHaveText(
      'Download failed',
    );
    await expect(download).toBeEnabled();
  });

  test('arrow keys scroll a focused text panel instead of switching files', async ({
    page,
  }) => {
    await setup(page);
    await openModal(page);
    await page
      .locator('[data-comment-id="1"]')
      .getByRole('button', { name: /notes\.md/ })
      .click();
    const viewer = viewerOf(page);
    const panel = viewer.getByTestId('viewer-markdown');
    await expect(panel).toBeVisible();
    await panel.focus();
    await page.keyboard.press('ArrowLeft');
    await expect(viewer.getByText('3 / 3')).toBeVisible();
    await viewer.getByRole('button', { name: 'Download' }).focus();
    await page.keyboard.press('ArrowLeft');
    await expect(viewer.getByText('2 / 3')).toBeVisible();
  });
});
