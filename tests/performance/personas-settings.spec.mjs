import { expect, test } from '@playwright/test';
import { mockApp } from './mockApp.mjs';

test('the Personas settings page lists personas and offers a create form', async ({
	page,
}) => {
	await mockApp(page);
	await page.route('**/api/personas*', (route) =>
		route.fulfill({
			json: {
				data: [
					{
						id: 'uuid-1',
						name: 'Reviewer',
						description: 'Reviews incoming tasks',
						avatar_url: null,
						system_prompt: 'You are a reviewer.',
						prompt_version: 2,
						archived_at: null,
						created_at: '2026-01-01T00:00:00Z',
						updated_at: '2026-01-01T00:00:00Z',
						owner: { id: 1, name: 'Test User' },
					},
				],
			},
		}),
	);

	await page.goto('/settings/personas');

	await expect(page.getByRole('heading', { name: 'Personas' })).toBeVisible();
	await expect(page.getByText('Reviewer')).toBeVisible();
	await expect(page.getByText('prompt v2')).toBeVisible();
	await expect(page.getByPlaceholder('Reviewer')).toBeVisible();
});

test('an empty persona list shows the empty state instead of an error', async ({
	page,
}) => {
	await mockApp(page);
	await page.goto('/settings/personas');

	await expect(page.getByRole('heading', { name: 'Personas' })).toBeVisible();
	await expect(page.getByText('No personas yet.')).toBeVisible();
});
