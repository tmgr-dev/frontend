import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { approvalMessage, signMessage, verifyMessage } from '../agentApproval';
import { buildBulkItems, type BulkTask } from '../agentApprovalBulk';

const fixtures = JSON.parse(
	readFileSync(
		join(__dirname, 'fixtures', 'agent-approval-fixtures.json'),
		'utf8',
	),
);

beforeAll(() => {
	if (!globalThis.crypto?.subtle)
		Object.defineProperty(globalThis, 'crypto', { value: webcrypto });
});

const tasks = [
	{ id: 1, title: 'One', description: 'a', workspaceId: 56, categoryId: 127 },
	{ id: 2, title: 'Two', description: null, workspaceId: 56, categoryId: 0 },
	{
		id: 3,
		title: 'Three',
		description: 'c\r\nd',
		workspaceId: 56,
		categoryId: 5,
	},
];

const build = async (list: BulkTask[] = tasks) => {
	const key = await globalThis.crypto.subtle.importKey(
		'jwk',
		{ ...fixtures.key.private_jwk, key_ops: ['sign'] },
		{ name: 'ECDSA', namedCurve: 'P-256' },
		false,
		['sign'],
	);
	return buildBulkItems({
		tasks: list,
		host: 'api.tmgr.dev',
		userId: 1,
		keyId: fixtures.key.key_id,
		signedAt: 1760000000,
		sign: (message) => signMessage(key, message),
	});
};

describe('buildBulkItems', () => {
	it('signs one distinct message per task and each verifies', async () => {
		const items = await build();
		expect(items).toHaveLength(3);
		expect(new Set(items.map((i) => i.signature)).size).toBe(3);
		for (const item of items) {
			const task = tasks.find((t) => t.id === item.task_id)!;
			const message = approvalMessage(
				'api.tmgr.dev',
				1,
				task.workspaceId,
				task.id,
				task.categoryId,
				item.content_sha256,
				fixtures.key.key_id,
				1760000000,
			);
			expect(item.kind).toBe('confirm');
			expect(item.category_id).toBe(task.categoryId);
			expect(
				await verifyMessage(fixtures.key.public_key, message, item.signature),
			).toBe(true);
		}
	});

	it('a signature does not verify against another task message', async () => {
		const items = await build();
		const other = approvalMessage(
			'api.tmgr.dev',
			1,
			56,
			2,
			0,
			items[1].content_sha256,
			fixtures.key.key_id,
			1760000000,
		);
		expect(
			await verifyMessage(fixtures.key.public_key, other, items[0].signature),
		).toBe(false);
	});

	it('changed text gives a different hash', async () => {
		const before = await build();
		const after = await build([
			{ ...tasks[0], title: 'One!' },
			...tasks.slice(1),
		]);
		expect(after[0].content_sha256).not.toBe(before[0].content_sha256);
		expect(after[1].content_sha256).toBe(before[1].content_sha256);
	});

	it('signs v2 with the repository hash for tasks with a set, v1 otherwise', async () => {
		const c = fixtures.cases_v2[0];
		const items = await build([
			{
				id: c.task_id,
				title: c.title,
				description: c.description,
				workspaceId: c.workspace_id,
				categoryId: c.category_id,
				repositories: c.repositories,
			},
			{ ...tasks[0], repositories: null },
		]);
		expect(items[0].repos_sha256).toBe(c.repos_sha256);
		expect(items[1].repos_sha256).toBeUndefined();
		const message = approvalMessage(
			'api.tmgr.dev',
			1,
			c.workspace_id,
			c.task_id,
			c.category_id,
			items[0].content_sha256,
			fixtures.key.key_id,
			1760000000,
			c.repos_sha256,
		);
		expect(message).toContain(':v2:');
		expect(
			await verifyMessage(fixtures.key.public_key, message, items[0].signature),
		).toBe(true);
	});

	it('refuses a non-canonical repository list', async () => {
		await expect(
			build([{ ...tasks[0], repositories: ['Acme/web'] }]),
		).rejects.toThrow();
	});
});
