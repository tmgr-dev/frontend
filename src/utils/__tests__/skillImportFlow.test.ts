import type { ScanResult, ScanSkill } from '@/utils/skillImport';
import { adoptMatches, buildRows, runImport } from '@/utils/skillImportFlow';

const skill = (over: Partial<ScanSkill> = {}): ScanSkill => ({
	key: 'claude:personal:review',
	dir_name: 'review',
	path: '~/.claude/skills/review',
	content: '---\nname: review\ndescription: Review code\n---\n# Review\nBody\n',
	hash: 'h1',
	size: 60,
	too_large: false,
	secret: false,
	secret_reason: null,
	symlink_target: null,
	supporting: [],
	supporting_more: 0,
	...over,
});

const scanOf = (...skills: ScanSkill[]): ScanResult => ({
	sources: [
		{
			id: 'claude',
			label: 'Claude Code',
			packages: [
				{ id: 'personal', name: 'Personal skills', version: null, skills },
			],
		},
	],
	skipped: [],
});

const noBody = async () => '';

describe('buildRows', () => {
	it('marks a fresh skill new and ticked', async () => {
		const rows = await buildRows(scanOf(skill()), [], {}, noBody);
		expect(rows[0].status).toBe('new');
		expect(rows[0].ticked).toBe(true);
		expect(rows[0].converted.slug).toBe('review');
	});

	it('treats an identical untracked library skill as up to date', async () => {
		const first = await buildRows(scanOf(skill()), [], {}, noBody);
		const rows = await buildRows(
			scanOf(skill()),
			[{ slug: 'review', version: 3 }],
			{},
			async () => first[0].converted.markdown,
		);
		expect(rows[0].status).toBe('up-to-date');
		expect(rows[0].ticked).toBe(false);
	});

	it('leaves a different untracked library skill as slug taken and unticked', async () => {
		const rows = await buildRows(
			scanOf(skill()),
			[{ slug: 'review', version: 3 }],
			{},
			async () => 'other',
		);
		expect(rows[0].status).toBe('slug-taken');
		expect(rows[0].ticked).toBe(false);
	});

	it('detects a changed source through the index and keeps the stored slug', async () => {
		const rows = await buildRows(
			scanOf(
				skill({
					hash: 'h2',
					content: skill().content.replace('name: review', 'name: renamed'),
				}),
			),
			[{ slug: 'review', version: 2 }],
			{
				'claude:personal:review': {
					slug: 'review',
					sourceHash: 'h1',
					libraryVersion: 2,
					importedAt: 't',
				},
			},
			noBody,
		);
		expect(rows[0].status).toBe('changed');
		expect(rows[0].converted.slug).toBe('review');
	});
});

describe('runImport', () => {
	it('puts ticked rows, records the index only for successes and reports failures', async () => {
		const rows = await buildRows(
			scanOf(
				skill(),
				skill({
					key: 'claude:personal:b',
					dir_name: 'b',
					hash: 'h2',
					content: '---\nname: b\ndescription: B\n---\nx\n',
				}),
			),
			[],
			{},
			noBody,
		);
		const put = jest.fn(async (slug: string) => {
			if (slug === 'b') throw { response: { data: { message: 'nope' } } };
			return { version: 1 };
		});
		const out = await runImport(rows, {
			listLibrary: async () => [],
			put,
			index: {},
			now: () => 'now',
		});
		expect(put).toHaveBeenCalledTimes(2);
		expect(out.results).toEqual([
			{ skillKey: 'claude:personal:review', ok: true },
			{ skillKey: 'claude:personal:b', ok: false, error: 'nope' },
		]);
		expect(Object.keys(out.index)).toEqual(['claude:personal:review']);
		expect(out.index['claude:personal:review']).toEqual({
			slug: 'review',
			sourceHash: 'h1',
			libraryVersion: 1,
			importedAt: 'now',
			contentHash: expect.any(String),
		});
	});

	it('imports nothing for blocked or unticked rows', async () => {
		const rows = await buildRows(
			scanOf(skill({ secret: true, content: '' })),
			[],
			{},
			noBody,
		);
		const put = jest.fn();
		const out = await runImport(rows, {
			listLibrary: async () => [],
			put,
			index: {},
		});
		expect(rows[0].status).toBe('blocked');
		expect(put).not.toHaveBeenCalled();
		expect(out.results).toEqual([]);
	});
});

describe('safety', () => {
	it('refuses a new row whose slug appeared in the library after the scan', async () => {
		const rows = await buildRows(scanOf(skill()), [], {}, noBody);
		const put = jest.fn();
		const out = await runImport(rows, {
			listLibrary: async () => [{ slug: 'review', version: 1 }],
			put,
			index: {},
		});
		expect(put).not.toHaveBeenCalled();
		expect(out.blocked).toEqual([
			{
				skillKey: 'claude:personal:review',
				reason: 'slug taken in the library since the scan',
			},
		]);
	});

	it('refuses to update when the library skill is no longer what was imported', async () => {
		const first = await buildRows(scanOf(skill()), [], {}, noBody);
		const done = await runImport(first, {
			listLibrary: async () => [],
			put: async () => ({ version: 1 }),
			index: {},
		});
		const rows = await buildRows(
			scanOf(skill({ hash: 'h2' })),
			[{ slug: 'review', version: 1 }],
			done.index,
			noBody,
		);
		expect(rows[0].status).toBe('changed');
		const put = jest.fn(async () => ({ version: 2 }));
		const out = await runImport(rows, {
			listLibrary: async () => [{ slug: 'review', version: 1 }],
			put,
			getBody: async () => 'something typed by hand',
			index: done.index,
		});
		expect(put).not.toHaveBeenCalled();
		expect(out.results[0].ok).toBe(false);
	});

	it('adopts an identical library skill into the index', async () => {
		const first = await buildRows(scanOf(skill()), [], {}, noBody);
		const library = [{ slug: 'review', version: 4 }];
		const rows = await buildRows(
			scanOf(skill()),
			library,
			{},
			async () => first[0].converted.markdown,
		);
		const index = adoptMatches(rows, library, {}, 'now');
		expect(index['claude:personal:review']).toMatchObject({
			slug: 'review',
			libraryVersion: 4,
			sourceHash: 'h1',
		});
	});
});
