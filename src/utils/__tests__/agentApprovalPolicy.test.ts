import {
	categoryIdOf,
	categoryLabel,
	expiringSoon,
	shouldAutoSign,
	splitInvisible,
	type AllowedEntry,
	type ApprovalState,
} from '../agentApprovalPolicy';

const text = { title: 'T', description: 'a\nb' };

describe('shouldAutoSign', () => {
	const states: ApprovalState[] = ['none', 'active', 'stale', 'revoked'];
	const cases: [ApprovalState, boolean, boolean, boolean, boolean, boolean][] =
		[];
	for (const state of states)
		for (const own of [true, false])
			for (const authored of [true, false])
				for (const owner of [true, false])
					for (const setting of [true, false]) {
						const expected =
							setting &&
							owner &&
							((state === 'none' && own && authored) ||
								state === 'active' ||
								state === 'stale');
						cases.push([state, own, authored, owner, setting, expected]);
					}
	it.each(cases)(
		'state %s sameId=%s ownerAuthored=%s ownerSave=%s setting=%s -> %s',
		(state, own, authored, owner, setting, expected) => {
			expect(
				shouldAutoSign({
					isOwnerSave: owner,
					authorId: own ? 7 : 8,
					meId: 7,
					state,
					settingEnabled: setting,
					ownerAuthored: authored,
					submitted: text,
					saved: text,
				}),
			).toBe(expected);
		},
	);

	const base = {
		submitted: text,
		saved: text,
		isOwnerSave: true,
		authorId: 7,
		meId: 7,
		settingEnabled: true,
		ownerAuthored: true,
	};

	it('does not sign a legacy or MCP-created task in state none', () => {
		expect(
			shouldAutoSign({ ...base, state: 'none', ownerAuthored: false }),
		).toBe(false);
	});

	it('does not sign when the server returned different text', () => {
		for (const state of ['none', 'active', 'stale'] as const) {
			expect(
				shouldAutoSign({
					...base,
					state,
					saved: { ...text, title: 'Swapped' },
				}),
			).toBe(false);
			expect(
				shouldAutoSign({
					...base,
					state,
					saved: { ...text, description: 'a\nb extra' },
				}),
			).toBe(false);
			expect(
				shouldAutoSign({
					...base,
					state,
					submitted: { title: 'T', description: null },
				}),
			).toBe(false);
		}
	});

	it('signs when the texts match', () => {
		expect(shouldAutoSign({ ...base, state: 'stale' })).toBe(true);
	});

	it('signs when only CRLF differs', () => {
		expect(
			shouldAutoSign({
				...base,
				state: 'stale',
				saved: { title: 'T', description: 'a\r\nb' },
			}),
		).toBe(true);
	});

	it('treats null and empty description as equal', () => {
		expect(
			shouldAutoSign({
				...base,
				state: 'stale',
				submitted: { title: 'T', description: null },
				saved: { title: 'T', description: '' },
			}),
		).toBe(true);
	});

	it('never signs a revoked approval', () => {
		expect(shouldAutoSign({ ...base, state: 'revoked' })).toBe(false);
	});

	it('does not treat missing ids as the author', () => {
		expect(
			shouldAutoSign({
				...base,
				authorId: undefined,
				meId: undefined,
				state: 'none',
			}),
		).toBe(false);
	});
});

describe('expiringSoon', () => {
	const now = Date.parse('2026-10-19T12:00:00Z');
	const entry = (code: string, expiresAt: string | null, expired = false) =>
		({
			workspaceCode: code,
			workspaceId: 1,
			expiresAt,
			expired,
			keys: 1,
			repos: 0,
		}) as AllowedEntry;

	it('selects entries expiring within 3 days only', () => {
		const list = [
			entry('soon', '2026-10-22T12:00:00Z'),
			entry('later', '2026-10-22T12:00:01Z'),
			entry('forever', null),
			entry('gone', '2026-10-18T00:00:00Z', true),
			entry('gone2', '2026-10-19T11:59:59Z'),
			entry('bad', 'nope'),
			entry('tomorrow', '2026-10-20T00:00:00Z'),
		];
		expect(expiringSoon(list, now).map((e) => e.workspaceCode)).toEqual([
			'soon',
			'tomorrow',
		]);
	});
});

describe('splitInvisible', () => {
	it('flags zero-width and bidi characters, keeps newlines and tabs', () => {
		const zw = String.fromCharCode(0x200b);
		const rlo = String.fromCharCode(0x202e);
		const pieces = splitInvisible(`a${zw}b\n\t${rlo}c`);
		expect(pieces).toEqual([
			{ text: 'a', flagged: false },
			{ text: '<U+200B>', flagged: true },
			{ text: 'b\n\t', flagged: false },
			{ text: '<U+202E>', flagged: true },
			{ text: 'c', flagged: false },
		]);
	});
});

describe('category display matches the signed id', () => {
	const cases: [any, number, string][] = [
		[{ category: { id: 127, code: 'TM', title: 'Tasks' } }, 127, 'TM - Tasks'],
		[{ project_category_id: 5, category: { id: 5, code: 'X' } }, 5, 'X'],
		[
			{ project_category_id: 9, category: { id: 5, code: 'X' } },
			9,
			'Category #9',
		],
		[{ category: 12 }, 12, 'Category #12'],
		[{ category: null }, 0, 'No category'],
		[{}, 0, 'No category'],
		[{ project_category_id: null, category: { id: 0 } }, 0, 'No category'],
	];
	it.each(cases)('%j -> %s / %s', (task, id, label) => {
		expect(categoryIdOf(task)).toBe(id);
		expect(categoryLabel(task)).toBe(label);
	});
});
