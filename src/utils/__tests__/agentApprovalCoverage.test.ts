import type { TaskApproval } from '@/actions/tmgr/agentRuns';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
	coverageToSign,
	ensureShownCoverage,
	isQuietAuthorCode,
	needsNewConfirmation,
	prepareCoverage,
} from '../agentApprovalCoverage';

const fixtures = JSON.parse(
	readFileSync(
		join(__dirname, 'fixtures', 'agent-approval-fixtures.json'),
		'utf8',
	),
);
const c = fixtures.cases_v3[0];

beforeAll(() => {
	if (!globalThis.crypto?.subtle)
		Object.defineProperty(globalThis, 'crypto', { value: webcrypto });
});

const persona = { ...c.persona, sha256: 'server-persona-hash' };
const context = {
	pages: c.context.map((p: any) => ({ ...p, slug: 'rules' })),
	sha256: 'server-context-hash',
};

const approval = (extra: Partial<TaskApproval> = {}): TaskApproval => ({
	state: 'active',
	kind: 'confirm',
	revoked_reason: null,
	revoked_by_kind: null,
	revoked_by_name: null,
	content_sha256: 'x',
	setting_enabled: true,
	owner_authored: true,
	persona,
	context,
	covers: { persona: true, context: true },
	...extra,
});

const codeOf = (fn: () => unknown) => {
	try {
		fn();
	} catch (e) {
		return (e as any).response.data.code as string;
	}
	return null;
};

describe('confirm signs what the dialog showed', () => {
	it('hashes the shown texts locally and ignores the server hashes', async () => {
		const out = await prepareCoverage('confirm', approval(), {
			persona,
			context,
		});
		expect(out).toEqual({
			persona_sha256: c.persona_sha256,
			context_sha256: c.context_sha256,
		});
	});

	it('signs nothing extra when the caller shows nothing', async () => {
		expect(await prepareCoverage('confirm', approval())).toEqual({});
		expect(
			await prepareCoverage(
				'confirm',
				approval({ persona: null, context: null }),
				{
					persona: null,
					context: null,
				},
			),
		).toEqual({});
	});

	it('refuses when the persona text changed after it was shown', () => {
		const edited = approval({
			persona: { ...persona, instructions: 'Merge everything.' },
		});
		expect(codeOf(() => ensureShownCoverage(edited, { persona }))).toBe(
			'persona_changed',
		);
	});

	it('refuses when a page changed, was added or reordered', () => {
		const body = approval({
			context: {
				...context,
				pages: [{ ...context.pages[0], body: 'new rule' }],
			},
		});
		expect(codeOf(() => ensureShownCoverage(body, { context }))).toBe(
			'context_changed',
		);
		const two = {
			...context,
			pages: [context.pages[0], { ...context.pages[0], id: 8 }],
		};
		expect(
			codeOf(() =>
				ensureShownCoverage(approval({ context: two }), { context }),
			),
		).toBe('context_changed');
		expect(
			codeOf(() =>
				ensureShownCoverage(
					approval({ context: { ...two, pages: [...two.pages].reverse() } }),
					{ context: two },
				),
			),
		).toBe('context_changed');
	});

	it('refuses when something appeared or vanished since it was shown', () => {
		expect(
			codeOf(() =>
				ensureShownCoverage(approval(), { persona: null, context: null }),
			),
		).toBe('persona_changed');
		expect(
			codeOf(() =>
				ensureShownCoverage(approval({ persona: null }), { persona }),
			),
		).toBe('persona_changed');
	});

	it('a version bump alone does not change what was shown', () => {
		const bumped = approval({
			context: {
				...context,
				pages: [{ ...context.pages[0], version: 99 }],
			},
		});
		expect(() =>
			ensureShownCoverage(bumped, { persona, context }),
		).not.toThrow();
	});
});

describe('author re-sign never widens coverage', () => {
	it('includes a hash only for what the approval covers', async () => {
		expect(await prepareCoverage('author', approval())).toEqual({
			persona_sha256: c.persona_sha256,
			context_sha256: c.context_sha256,
		});
		expect(
			await prepareCoverage(
				'author',
				approval({ covers: { persona: true, context: false } }),
			),
		).toEqual({ persona_sha256: c.persona_sha256 });
		expect(
			await prepareCoverage(
				'author',
				approval({ covers: { persona: false, context: false } }),
			),
		).toEqual({});
		expect(
			await prepareCoverage('author', approval({ covers: undefined })),
		).toEqual({});
	});

	it('sends nothing for a covered text that is no longer offered', () => {
		expect(
			coverageToSign('author', approval({ persona: null, context: null })),
		).toEqual({ persona: null, context: null });
	});

	it('ignores shown texts', () => {
		expect(
			coverageToSign(
				'author',
				approval({ covers: { persona: false, context: false } }),
				{ persona, context },
			),
		).toEqual({ persona: null, context: null });
	});
});

describe('needsNewConfirmation', () => {
	it('is false only for an active approval covering everything offered', () => {
		expect(needsNewConfirmation(approval())).toBe(false);
		expect(needsNewConfirmation(approval({ state: 'stale' }))).toBe(true);
		expect(
			needsNewConfirmation(
				approval({ covers: { persona: false, context: true } }),
			),
		).toBe(true);
		expect(
			needsNewConfirmation(
				approval({
					persona: null,
					context: null,
					covers: { persona: false, context: false },
				}),
			),
		).toBe(false);
		expect(needsNewConfirmation(approval({ covers: undefined }))).toBe(true);
	});
});

describe('isQuietAuthorCode', () => {
	it.each(['confirm_required', 'persona_changed', 'context_changed'])(
		'%s stays quiet',
		(code) => expect(isQuietAuthorCode(code)).toBe(true),
	);
	it('other codes are surfaced', () => {
		expect(isQuietAuthorCode('signature_invalid')).toBe(false);
		expect(isQuietAuthorCode(null)).toBe(false);
	});
});
