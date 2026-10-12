import { expect, test } from '@playwright/test';

import { mockApp } from './mockApp.mjs';

const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

const openProfile = async (page) => {
  await page.addInitScript(() =>
    sessionStorage.setItem('tmgr:tours:auto', 'true'),
  );
  await mockApp(page);
  const calls = { presign: [], put: [], store: [] };
  await page.route('**/api/users/1/avatar/signed-url', (route) =>
    route.fulfill({ status: 404, json: { message: 'no avatar' } }),
  );
  await page.route('**/api/files/presign-upload', (route) => {
    const body = route.request().postDataJSON();
    calls.presign.push(body);
    const origin = new URL(route.request().url()).origin;
    return route.fulfill({
      json: {
        data: {
          key: 'avatars/1/fixture-key',
          upload_url: `${origin}/__storage/avatar?sig=1`,
          content_type: body.content_type,
        },
      },
    });
  });
  await page.route('**/__storage/avatar*', (route) => {
    const request = route.request();
    calls.put.push({
      method: request.method(),
      contentType: request.headers()['content-type'],
      body: request.postDataBuffer(),
    });
    return route.fulfill({ status: 200, body: '' });
  });
  await page.route('**/api/user/store-avatar', (route) => {
    calls.store.push(route.request().postDataJSON());
    return route.fulfill({ json: { data: {} } });
  });
  await page.goto('/settings?tab=profile');
  await expect(
    page.getByRole('button', { name: 'Change picture' }),
  ).toBeVisible();
  return calls;
};

const pngFile = async (page) => ({
  name: 'photo.png',
  mimeType: 'image/png',
  buffer: await page.screenshot({
    clip: { x: 0, y: 0, width: 64, height: 40 },
  }),
});

const cropDialog = (page) => page.getByRole('dialog', { name: 'Crop avatar' });

const pick = (page, file) =>
  page.locator('input[type="file"][accept^="image/"]').setInputFiles(file);

const expectNoUpload = (calls) =>
  expect({
    presign: calls.presign.length,
    put: calls.put.length,
    store: calls.store.length,
  }).toEqual({ presign: 0, put: 0, store: 0 });

test('picking an image opens the crop modal with a circular mask', async ({
  page,
}) => {
  const calls = await openProfile(page);
  await pick(page, await pngFile(page));

  const modal = cropDialog(page);
  await expect(modal).toBeVisible();
  await expect(modal.getByText('Crop avatar')).toBeVisible();
  await expect(modal.getByTestId('avatar-crop-confirm')).toBeEnabled();
  const mask = modal.locator('.rounded-full').first();
  await expect(mask).toBeVisible();
  const box = await mask.boundingBox();
  expect(box.width).toBeGreaterThan(200);
  expect(Math.round(box.width)).toBe(Math.round(box.height));
  expect(
    await mask.evaluate((el) => parseFloat(getComputedStyle(el).borderRadius)),
  ).toBeGreaterThanOrEqual(box.width / 2);
  expectNoUpload(calls);
});

test('Cancel in the crop modal uploads nothing', async ({ page }) => {
  const calls = await openProfile(page);
  await pick(page, await pngFile(page));
  const modal = cropDialog(page);
  await expect(modal.getByTestId('avatar-crop-confirm')).toBeEnabled();

  await modal.getByRole('button', { name: 'Cancel' }).click();
  await expect(modal).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Change picture' }),
  ).toBeEnabled();
  expectNoUpload(calls);
});

test('Confirm uploads one square image through presign, put and store', async ({
  page,
}) => {
  const calls = await openProfile(page);
  await pick(page, await pngFile(page));
  const modal = cropDialog(page);
  await modal.getByTestId('avatar-crop-confirm').click();
  await expect(modal).toHaveCount(0);

  await expect.poll(() => calls.store.length).toBe(1);
  await expect(
    page.getByRole('button', { name: 'Change picture' }),
  ).toBeEnabled();
  expect(calls.presign).toHaveLength(1);
  expect(calls.put).toHaveLength(1);
  expect(calls.store).toHaveLength(1);

  expect(calls.presign[0].content_type).toMatch(/^image\//);
  const [put] = calls.put;
  expect(put.method).toBe('PUT');
  expect(put.contentType).toMatch(/^image\//);
  expect([...put.body.subarray(0, 8)]).toEqual(PNG_MAGIC);
  const width = put.body.readUInt32BE(16);
  const height = put.body.readUInt32BE(20);
  expect(width).toBeGreaterThan(0);
  expect(width).toBe(height);
  expect(calls.presign[0].size_bytes).toBe(put.body.length);
  expect(calls.store[0]).toEqual({ path: 'avatars/1/fixture-key' });
  await expect(
    page.getByText(/Could not (upload|read)|Pick an image file/),
  ).toHaveCount(0);
});

test('a non-image file shows an error and uploads nothing', async ({
  page,
}) => {
  const calls = await openProfile(page);
  await pick(page, {
    name: 'notes.txt',
    mimeType: 'text/plain',
    buffer: Buffer.from('not a picture'),
  });

  await expect(page.getByText('Pick an image file')).toBeVisible();
  await expect(cropDialog(page)).toHaveCount(0);
  expectNoUpload(calls);
});

test('a broken image shows an error and uploads nothing', async ({ page }) => {
  const calls = await openProfile(page);
  await pick(page, {
    name: 'broken.png',
    mimeType: 'image/png',
    buffer: Buffer.from('these bytes are not a png at all'),
  });

  await expect(page.getByText('Could not read that image')).toBeVisible();
  await expect(cropDialog(page)).toHaveCount(0);
  expectNoUpload(calls);
});
