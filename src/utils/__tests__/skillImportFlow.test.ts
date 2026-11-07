import {
	EMPTY_FILES_SHA256 as EMPTY,
	type ScanResult,
	type ScanSkill,
} from '@/utils/skillImport';
import { adoptMatches, buildRows, runImport } from '@/utils/skillImportFlow';
import { createHash } from 'crypto';

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
	files: [],
	files_sha256: EMPTY,
	files_blocked: null,
	files_truncated: false,
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
			filesSha256: EMPTY,
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
		expect(out.index['claude:personal:review']).toBeUndefined();
	});

	it('accepts trailing whitespace differences in the library body', async () => {
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
		const put = jest.fn(async () => ({ version: 2 }));
		const out = await runImport(rows, {
			listLibrary: async () => [{ slug: 'review', version: 1 }],
			put,
			getBody: async () => first[0].converted.markdown + '\n\n',
			index: done.index,
		});
		expect(put).toHaveBeenCalled();
		expect(out.results[0].ok).toBe(true);
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

describe('skill files in the import', () => {
	const sha = (c: string) => createHash('sha256').update(c).digest('hex');
	const fileEntry = (path: string, content: string) => ({
		path,
		content,
		size: content.length,
		sha256: sha(content),
	});
	const setSha = (files: { path: string; sha256: string }[]) =>
		createHash('sha256')
			.update(
				[...files]
					.sort((a, b) => (a.path < b.path ? -1 : 1))
					.map((f) => `${f.path}\0${f.sha256}\n`)
					.join(''),
			)
			.digest('hex');
	const entries = [fileEntry('run.sh', 'echo hi'), fileEntry('refs/g.md', 'g')];
	const filesSha = setSha(entries);
	const scanFiles = entries.map(({ path, size, sha256 }) => ({
		path,
		size,
		sha256,
		status: 'ok' as const,
		reason: null,
	}));
	const withFiles = (over: Partial<ScanSkill> = {}) =>
		skill({ files: scanFiles, files_sha256: filesSha, ...over });
	const read = (over = {}) =>
		jest.fn(async (keys: string[]) => [
			{
				key: keys[0],
				ok: true,
				error: null,
				files: entries,
				files_sha256: filesSha,
				...over,
			},
		]);

	it('uploads body and files in one put with the expected library hash and stores the hash in the index', async () => {
		const rows = await buildRows(scanOf(withFiles()), [], {}, noBody);
		const put = jest.fn(async () => ({ version: 1 }));
		const readFiles = read();
		const out = await runImport(rows, {
			listLibrary: async () => [],
			put,
			readFiles,
			index: {},
			now: () => 'now',
		});
		expect(readFiles).toHaveBeenCalledWith(['claude:personal:review']);
		expect(put).toHaveBeenCalledTimes(1);
		expect(put).toHaveBeenCalledWith(
			'review',
			rows[0].converted.markdown,
			{ files: entries, sha256: filesSha },
			EMPTY,
		);
		expect(out.index['claude:personal:review'].filesSha256).toBe(filesSha);
	});

	it('sends the library files hash of an existing skill as the expected one', async () => {
		const first = await buildRows(scanOf(withFiles()), [], {}, noBody);
		const library = [
			{ slug: 'review', version: 2, filesSha256: 'f'.repeat(64) },
		];
		const rows = await buildRows(
			scanOf(withFiles()),
			library,
			{},
			async () => first[0].converted.markdown,
		);
		expect(rows[0].status).toBe('slug-taken');
		rows[0].ticked = true;
		const put = jest.fn(async (..._args: unknown[]) => ({ version: 3 }));
		await runImport(rows, {
			listLibrary: async () => library,
			put,
			readFiles: read(),
			index: {},
		});
		expect(put.mock.calls[0][3]).toBe('f'.repeat(64));
	});

	it('clears the files of a skill that has none', async () => {
		const rows = await buildRows(scanOf(skill()), [], {}, noBody);
		const put = jest.fn(async (..._args: unknown[]) => ({ version: 1 }));
		const readFiles = jest.fn();
		await runImport(rows, {
			listLibrary: async () => [],
			put,
			readFiles,
			index: {},
		});
		expect(readFiles).not.toHaveBeenCalled();
		expect(put.mock.calls[0][2]).toEqual({ files: [], sha256: EMPTY });
	});

	it('does not put a skill whose files changed since the preview', async () => {
		const rows = await buildRows(scanOf(withFiles()), [], {}, noBody);
		const put = jest.fn();
		for (const changed of [
			{ ok: false, error: 'changed since preview, rescan', files: [] },
			{ files_sha256: 'e'.repeat(64) },
		]) {
			const out = await runImport(rows, {
				listLibrary: async () => [],
				put,
				readFiles: read(changed),
				index: {},
			});
			expect(out.results).toEqual([
				{
					skillKey: 'claude:personal:review',
					ok: false,
					error: 'changed since preview, rescan',
				},
			]);
			expect(out.index).toEqual({});
		}
		expect(put).not.toHaveBeenCalled();
	});

	it('blocks a skill with a secret file: body is not imported either', async () => {
		const rows = await buildRows(
			scanOf(
				withFiles({
					files_blocked: 'run.sh looks like it contains a secret',
					files: [
						{
							path: 'run.sh',
							size: 5,
							status: 'secret',
							reason: 'contains a GitHub token',
							sha256: null,
						},
					],
				}),
			),
			[],
			{},
			noBody,
		);
		expect(rows[0].status).toBe('blocked');
		const put = jest.fn();
		const out = await runImport(rows, {
			listLibrary: async () => [],
			put,
			index: {},
		});
		expect(put).not.toHaveBeenCalled();
		expect(out.results).toEqual([]);
	});

	it('reports a files hash conflict from the server', async () => {
		const rows = await buildRows(scanOf(withFiles()), [], {}, noBody);
		const out = await runImport(rows, {
			listLibrary: async () => [],
			put: async () => {
				throw { response: { status: 409, data: { message: 'x' } } };
			},
			readFiles: read(),
			index: {},
		});
		expect(out.results[0].error).toMatch(/Files changed in the library/);
	});

	it('a changed file alone gives update available, old index entries count as no files', async () => {
		const entry = {
			slug: 'review',
			sourceHash: 'h1',
			libraryVersion: 2,
			importedAt: 't',
		};
		const lib = [{ slug: 'review', version: 2 }];
		const index = { 'claude:personal:review': entry };
		expect(
			(await buildRows(scanOf(skill()), lib, index, noBody))[0].status,
		).toBe('up-to-date');
		expect(
			(await buildRows(scanOf(withFiles()), lib, index, noBody))[0].status,
		).toBe('changed');
		const stored = {
			'claude:personal:review': { ...entry, filesSha256: filesSha },
		};
		expect(
			(await buildRows(scanOf(withFiles()), lib, stored, noBody))[0].status,
		).toBe('up-to-date');
		expect(
			(
				await buildRows(
					scanOf(withFiles({ files_sha256: 'd'.repeat(64) })),
					lib,
					stored,
					noBody,
				)
			)[0].status,
		).toBe('changed');
	});
});
