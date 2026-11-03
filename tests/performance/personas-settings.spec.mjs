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

test('editing a persona saves its owner notification setting', async ({
  page,
}) => {
  await mockApp(page);
  let saved = null;
  await page.route('**/api/personas/uuid-1', (route) => {
    saved = route.request().postDataJSON();
    return route.fulfill({ json: { data: {} } });
  });
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
            owner_notifications: 'none',
            prompt_version: 1,
            archived_at: null,
            created_at: '2026-01-01T00:00:00Z',
            updated_at: '2026-01-01T00:00:00Z',
            owner: { id: 1, name: 'Test User' },
            can_edit: true,
          },
        ],
      },
    }),
  );

  await page.goto('/settings/personas');
  const select = page.getByRole('combobox', {
    name: "Notify me about this persona's activity",
  });
  await expect(select).toHaveText('All');

  await page.getByRole('button', { name: 'Edit' }).first().click();
  await expect(select).toHaveText('Never');

  await select.click();
  await page.getByRole('option', { name: 'Only when it asks for me' }).click();
  await page.getByRole('button', { name: 'Save' }).click();

  await expect.poll(() => saved?.owner_notifications).toBe('mentions');
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
              id: 1,
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
                id: 1,
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

  await expect(
    page.getByRole('heading', { name: 'Connect an agent' }),
  ).toBeVisible();
  await page.getByPlaceholder('Claude Code on my laptop').fill('Claude Code');
  await page.getByRole('button', { name: 'Issue token' }).click();

  await expect(page.getByText(secret)).toBeVisible();
  await expect(page.getByText(/won't see it again/i)).toBeVisible();
  await expect(page.getByText('${TMGR_PERSONA_TOKEN}')).toBeVisible();

  await page.getByRole('button', { name: 'Done' }).click();

  await expect(page.getByText(secret)).not.toBeVisible();
});

test('the shared skills library can create, attach, flag overrides and delete', async ({
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
            can_edit: true,
          },
        ],
      },
    }),
  );
  await page.route('**/api/personas/uuid-1/skills*', (route) =>
    route.fulfill({ json: { data: [] } }),
  );

  const entry = (slug, title) => ({
    slug,
    title,
    when: `when ${slug}`,
    actions: [],
    version: 1,
    updated_at: '2026-01-01T00:00:00Z',
  });
  const library = [entry('triage', 'Triage')];
  let attached = [];
  const puts = [];

  const summary = (skill) => ({
    ...skill,
    attached_personas: attached
      .filter((a) => a.slug === skill.slug)
      .map(() => ({ id: 'uuid-1', name: 'Reviewer', overridden: true })),
  });

  await page.route('**/api/shared-skills**', async (route) => {
    const request = route.request();
    const slug = new URL(request.url()).pathname.split('/').pop();
    if (request.method() === 'DELETE') {
      const index = library.findIndex((s) => s.slug === slug);
      const detached = attached.some((a) => a.slug === slug)
        ? [{ id: 'uuid-1', name: 'Reviewer' }]
        : [];
      library.splice(index, 1);
      attached = attached.filter((a) => a.slug !== slug);
      return route.fulfill({ json: { data: { detached_personas: detached } } });
    }
    if (request.method() === 'PUT') {
      puts.push(slug);
      const body = request.postDataJSON().body;
      const title = /^title:\s*(.+)$/m.exec(body)?.[1] ?? slug;
      library.push(entry(slug, title));
      return route.fulfill({ json: { data: entry(slug, title) } });
    }
    return route.fulfill({ json: { data: library.map(summary) } });
  });
  await page.route('**/api/personas/uuid-1/shared-skills**', async (route) => {
    const request = route.request();
    const slug = new URL(request.url()).pathname.split('/').pop();
    if (request.method() === 'PUT') {
      attached.push(entry(slug, library.find((s) => s.slug === slug).title));
      return route.fulfill({ json: { data: attached.at(-1) } });
    }
    if (request.method() === 'DELETE') {
      attached = attached.filter((a) => a.slug !== slug);
      return route.fulfill({ status: 204, body: '' });
    }
    return route.fulfill({
      json: { data: attached.map((a) => ({ ...a, overridden: true })) },
    });
  });

  await page.goto('/settings/personas');

  const libraryPanel = page.getByTestId('skills-library');
  await expect(libraryPanel.getByText('Triage', { exact: true })).toBeVisible();
  await expect(libraryPanel.getByText('Not attached')).toBeVisible();

  await libraryPanel
    .getByRole('textbox')
    .fill(
      '---\nslug: review\ntitle: Review\nwhen: reviewing a change\nactions: []\n---\nCheck the diff.\n',
    );
  await libraryPanel.getByRole('button', { name: 'Create skill' }).click();
  await expect.poll(() => puts).toEqual(['review']);
  await expect(libraryPanel.getByText('Review', { exact: true })).toBeVisible();

  await page.getByText('Skills', { exact: true }).first().click();
  const panel = page.getByTestId('persona-shared-skills');
  await expect(panel.getByText('No library skills attached.')).toBeVisible();

  await panel.getByRole('combobox', { name: 'Attach a library skill' }).click();
  await page.getByRole('option', { name: 'Triage (triage)' }).click();

  await expect(panel.getByText('v1 · when triage')).toBeVisible();
  await expect(panel.getByTestId('shared-skill-overridden')).toHaveText(
    "Overridden by this persona's own skill",
  );
  await expect(libraryPanel.getByText('Reviewer (overridden)')).toBeVisible();

  let message = '';
  page.once('dialog', (dialog) => {
    message = dialog.message();
    dialog.accept();
  });
  await libraryPanel
    .locator('div.rounded', { hasText: 'Triage' })
    .getByRole('button', { name: 'Delete' })
    .click();

  await expect.poll(() => message).toContain('Attached to Reviewer');
  await expect(
    page.getByText('Skill deleted and detached from 1 persona'),
  ).toBeVisible();
  await expect(panel.getByText('No library skills attached.')).toBeVisible();
});
