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

test('issuing a persona token shows the secret once, then clears it on close', async ({
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
						description: null,
						avatar_url: null,
						system_prompt: '',
						prompt_version: 1,
						archived_at: null,
						created_at: '2026-01-01T00:00:00Z',
						updated_at: '2026-01-01T00:00:00Z',
						owner: { id: 1, name: 'Test User' },
					},
				],
			},
		}),
	);
	await page.route('**/api/workspaces/1/personas*', (route) =>
		route.fulfill({
			json: {
				data: [
					{
						persona: {
							id: 'uuid-1',
							name: 'Reviewer',
							description: null,
							avatar_url: null,
							archived: false,
							owner: { id: 1, name: 'Test User' },
						},
						workspace_id: 1,
						permissions: ['tasks:read'],
						blocked: false,
						blocked_at: null,
						effective_permissions: ['tasks:read'],
					},
				],
			},
		}),
	);

	const secret = 'tmgrp_supersecretvalue1234567890';
	let issued = false;
	await page.route('**/api/personas/uuid-1/tokens*', async (route) => {
		if (route.request().method() === 'POST') {
			issued = true;
			await route.fulfill({
				json: {
					data: {
						token: {
							id: 'tok-1',
							persona_id: 'uuid-1',
							workspace_id: 1,
							workspace_name: 'Demo',
							prefix: 'tmgrp_ab12',
							label: 'Claude Code',
							expires_at: '2026-04-01T00:00:00Z',
							last_used_at: null,
							revoked_at: null,
							created_at: '2026-01-01T00:00:00Z',
						},
						secret,
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
								id: 'tok-1',
								persona_id: 'uuid-1',
								workspace_id: 1,
								workspace_name: 'Demo',
								prefix: 'tmgrp_ab12',
								label: 'Claude Code',
								expires_at: '2026-04-01T00:00:00Z',
								last_used_at: null,
								revoked_at: null,
								created_at: '2026-01-01T00:00:00Z',
							},
						]
					: [],
			},
		});
	});

	await page.goto('/settings/personas');
	await expect(page.getByText('Reviewer')).toBeVisible();

	await page.getByText('Agent connections', { exact: true }).click();
	await page.getByRole('button', { name: 'Connect an agent' }).click();

	await expect(page.getByRole('heading', { name: 'Connect an agent' })).toBeVisible();
	await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code');
	await page.getByRole('button', { name: 'Issue token' }).click();

	await expect(page.getByText(secret)).toBeVisible();
	await expect(page.getByText(/won't see it again/i)).toBeVisible();
	await expect(page.getByText('${TMGR_PERSONA_TOKEN}')).toBeVisible();

	await page.getByRole('button', { name: 'Done' }).click();

	await expect(page.getByText(secret)).not.toBeVisible();
});
