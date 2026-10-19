import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
	apiHostOf,
	approvalMessage,
	contentSha256,
	fromBase64,
	signMessage,
	toBase64,
	verifyMessage,
} from '../agentApproval';
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

const importPrivate = () =>
	globalThis.crypto.subtle.importKey(
		'jwk',
		{ ...fixtures.key.private_jwk, key_ops: ['sign'] },
		{ name: 'ECDSA', namedCurve: 'P-256' },
		false,
		['sign'],
	);

describe('agent approval fixtures', () => {
	it.each((fixtures.cases as any[]).map((c, i) => [i, c] as [number, any]))(
		'case %i hash and message match',
		async (_i: number, c: any) => {
			const sha = await contentSha256(c.title, c.description);
			expect(sha).toBe(c.content_sha256);
			expect(
				approvalMessage(
					c.host,
					c.user_id,
					c.workspace_id,
					c.task_id,
					c.category_id,
					sha,
					fixtures.key.key_id,
					c.signed_at,
				),
			).toBe(c.message);
		},
	);

	it.each((fixtures.cases as any[]).map((c, i) => [i, c] as [number, any]))(
		'case %i fixture signature verifies, tampered fails',
		async (_i: number, c: any) => {
			expect(
				await verifyMessage(fixtures.key.public_key, c.message, c.signature),
			).toBe(true);
			expect(
				await verifyMessage(
					fixtures.key.public_key,
					c.message,
					c.tampered_signature,
				),
			).toBe(false);
			expect(
				await verifyMessage(fixtures.other_public_key, c.message, c.signature),
			).toBe(false);
		},
	);

	it('signing with the fixture private key verifies and is raw r||s', async () => {
		const key = await importPrivate();
		const c = fixtures.cases[2];
		const signature = await signMessage(key, c.message);
		expect(fromBase64(signature)).toHaveLength(64);
		expect(
			await verifyMessage(fixtures.key.public_key, c.message, signature),
		).toBe(true);
		expect(
			await verifyMessage(fixtures.key.public_key, c.message + 'x', signature),
		).toBe(false);
	});

	it('a tampered fresh signature fails', async () => {
		const key = await importPrivate();
		const c = fixtures.cases[0];
		const bytes = fromBase64(await signMessage(key, c.message));
		bytes[10] ^= 1;
		expect(
			await verifyMessage(fixtures.key.public_key, c.message, toBase64(bytes)),
		).toBe(false);
	});
});

describe('apiHostOf', () => {
	it('returns the lowercase authority', () => {
		expect(apiHostOf('https://API.tmgr.dev/api/')).toBe('api.tmgr.dev');
		expect(apiHostOf('http://localhost:8080/api/')).toBe('localhost:8080');
		expect(apiHostOf('/api/', 'https://tmgr.dev')).toBe('tmgr.dev');
	});
});
