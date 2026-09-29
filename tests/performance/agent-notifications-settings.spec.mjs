import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('the Agent notifications settings page lists tokens', async ({
	page,
}) => {
	await mockApp(page);
	await page.route('**/api/notify-tokens*', (route) =>
		route.fulfill({
			json: {
				data: [
					{
						id: 1,
						label: 'Claude Code',
						prefix: 'tmgrn_ab12',
						created_at: '2026-01-01T00:00:00Z',
						last_used_at: null,
					},
				],
			},
		}),
	);

	await page.goto('/settings/agent-notifications');

	await expect(
		page.getByRole('heading', { name: 'Agent notifications' }),
	).toBeVisible();
	await expect(page.getByText('Claude Code', { exact: true })).toBeVisible();
	await expect(page.getByText(/tmgrn_ab12/)).toBeVisible();
});

test('an empty token list shows the empty state instead of an error', async ({
	page,
}) => {
	await mockApp(page);
	await page.route('**/api/notify-tokens*', (route) =>
		route.fulfill({ json: { data: [] } }),
	);

	await page.goto('/settings/agent-notifications');

	await expect(
		page.getByRole('heading', { name: 'Agent notifications' }),
	).toBeVisible();
	await expect(
		page.getByText('No agent notification tokens yet.'),
	).toBeVisible();
});

test('creating a token shows the secret once, then clears it on Done', async ({
	page,
}) => {
	await mockApp(page);
	const secret = 'tmgrn_supersecretvalue1234567890123456789';
	let issued = false;
	await page.route('**/api/notify-tokens*', async (route) => {
		if (route.request().method() === 'POST') {
			issued = true;
			await route.fulfill({
				json: {
					data: {
						id: 1,
						label: 'Claude Code',
						prefix: secret.slice(0, 10),
						created_at: '2026-01-01T00:00:00Z',
						last_used_at: null,
						token: secret,
					},
				},
			});
			return;
		}
		await route.fulfill({
			json: {
				data: issued
					? [
							{
								id: 1,
								label: 'Claude Code',
								prefix: secret.slice(0, 10),
								created_at: '2026-01-01T00:00:00Z',
								last_used_at: null,
							},
						]
					: [],
			},
		});
	});

	await page.goto('/settings/agent-notifications');
	await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code');
	await page.getByRole('button', { name: 'Create token' }).click();

	await expect(page.getByText(secret)).toBeVisible();
	await expect(page.getByText(/won't see it again/i)).toBeVisible();

	await page.getByRole('button', { name: 'Done' }).click();

	await expect(page.getByText(secret)).not.toBeVisible();
});
