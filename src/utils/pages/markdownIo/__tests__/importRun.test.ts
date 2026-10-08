import { buildImportPlan } from '../importPlan';
import { runImportWith } from '../importRun';
import type { ConflictPolicy } from '../types';
import { createFakeApi, file, seedPage, text } from './fakeApi';

const run = async (
	api: ReturnType<typeof createFakeApi>,
	files: ReturnType<typeof file>[],
	policy: ConflictPolicy = 'rename',
	parentId: number | null = null,
) => {
	const plan = buildImportPlan(files, {
		existingTitles: await api.listChildTitles(parentId),
	});
	return runImportWith(api, plan, { parentId, policy, workspaceCode: 'acme' });
};

const bodyOf = (api: ReturnType<typeof createFakeApi>, title: string) =>
	[...api.pages.values()].find((p) => p.title === title)!.body;

describe('runImportWith', () => {
	it('creates parents first and rewrites page and file links', async () => {
		const api = createFakeApi();
		const result = await run(api, [
			file(
				'Top.md',
				'[child](Top/Child.md#sec) [[Child|alias]] ![[pic.png|200]]',
			),
			file('Top/Child.md', '[back](../Top.md) ![x](../pic.png) [doc](doc.pdf)'),
			file('Top/doc.pdf', new Uint8Array([9])),
			file('pic.png', new Uint8Array([1])),
		]);
		expect(result.error).toBeNull();
		expect(result.created.map((c) => c.title)).toEqual(['Top', 'Child']);
		const [top, child] = result.created;
		expect(api.pages.get(child.id)!.parent_id).toBe(top.id);
		expect(bodyOf(api, 'Top')).toBe(
			`[child](tmgr://page/${child.id}) [alias](tmgr://page/${
				child.id
			}) ![](tmgr://file/${
				api.uploads.find((u) => u.pageId === top.id)!.fileId
			})`,
		);
		const childFiles = api.uploads.filter((u) => u.pageId === child.id);
		expect(childFiles.map((u) => u.file.name).sort()).toEqual([
			'doc.pdf',
			'pic.png',
		]);
		const ids = Object.fromEntries(
			childFiles.map((u) => [u.file.name, u.fileId]),
		);
		expect(bodyOf(api, 'Child')).toBe(
			`[back](tmgr://page/${top.id}) ![x](tmgr://file/${ids['pic.png']}) [doc](tmgr://file/${ids['doc.pdf']})`,
		);
		expect(api.uploads.find((u) => u.file.name === 'pic.png')!.file.mime).toBe(
			'image/png',
		);
	});

	it('reports progress and finishes once', async () => {
		const api = createFakeApi();
		const finish = jest.fn();
		api.finish = finish;
		const events: number[] = [];
		const plan = buildImportPlan(
			[file('A.md', '[b](B.md)'), file('B.md', 'b')],
			{
				existingTitles: [],
			},
		);
		await runImportWith(api, plan, {
			parentId: null,
			policy: 'rename',
			workspaceCode: 'acme',
			onProgress: (p) => events.push(p.done),
		});
		expect(finish).toHaveBeenCalledTimes(1);
		expect(events[0]).toBe(0);
		expect(events[events.length - 1]).toBe(3);
	});

	describe('conflict policies', () => {
		const files = () => [
			file('Guide.md', 'g'),
			file('Guide/Inner.md', 'i'),
			file('Fresh.md', 'f'),
		];

		it('rename adds a numeric suffix', async () => {
			const api = createFakeApi();
			seedPage(api, 'Guide');
			seedPage(api, 'guide (2)');
			const result = await run(api, files(), 'rename');
			expect(result.created.map((c) => c.title)).toEqual([
				'Fresh',
				'Guide (3)',
				'Inner',
			]);
		});

		it('skip drops the conflicting subtree', async () => {
			const api = createFakeApi();
			seedPage(api, 'Guide');
			const result = await run(api, files(), 'skip');
			expect(result.created.map((c) => c.title)).toEqual(['Fresh']);
			expect(result.skipped).toEqual(['Guide', 'Inner']);
		});

		it('import creates a duplicate title', async () => {
			const api = createFakeApi();
			seedPage(api, 'Guide');
			const result = await run(api, files(), 'import');
			expect(result.created.map((c) => c.title)).toEqual([
				'Fresh',
				'Guide',
				'Inner',
			]);
		});

		it('compares against the chosen parent only', async () => {
			const api = createFakeApi();
			const parent = seedPage(api, 'Parent');
			seedPage(api, 'Guide', parent.id);
			const result = await run(api, files(), 'rename', null);
			expect(result.created.map((c) => c.title)).toContain('Guide');
			const nested = await run(
				api,
				[file('Guide.md', 'g')],
				'rename',
				parent.id,
			);
			expect(nested.created[0].title).toBe('Guide (2)');
		});
	});

	it('stops at the first creation failure and lists what was created', async () => {
		const api = createFakeApi();
		api.failCreate = (title) => title === 'B';
		const result = await run(api, [
			file('A.md', 'a'),
			file('B.md', 'b'),
			file('C.md', 'c'),
		]);
		expect(result.created.map((c) => c.title)).toEqual(['A']);
		expect(result.error).toMatch(/Could not create "B"/);
	});

	it('turns an asset upload failure into a warning and keeps the link', async () => {
		const api = createFakeApi();
		api.failUpload = () => true;
		const result = await run(api, [
			file('A.md', '![p](p.png)'),
			file('p.png', new Uint8Array([1])),
		]);
		expect(result.error).toBeNull();
		expect(bodyOf(api, 'A')).toBe('![p](p.png)');
		expect(
			result.warnings.some((w) => /Could not upload/.test(w.message)),
		).toBe(true);
	});

	it('retries pass 2 once on a version conflict with a fresh version', async () => {
		const api = createFakeApi();
		const plan = buildImportPlan(
			[file('A.md', '[b](B.md)'), file('B.md', 'b')],
			{
				existingTitles: [],
			},
		);
		api.conflictOnce.add(1);
		const result = await runImportWith(api, plan, {
			parentId: null,
			policy: 'rename',
			workspaceCode: 'acme',
		});
		expect(result.warnings).toEqual([]);
		expect(bodyOf(api, 'A')).toBe('[b](tmgr://page/2)');
		expect(api.updates).toEqual([
			{ id: 1, version: 2, body: '[b](tmgr://page/2)' },
		]);
	});

	describe('tmgr references', () => {
		const withFm = (workspace: string | null, body: string) =>
			workspace
				? `---\ntitle: P\ntmgr:\n  workspace: ${workspace}\n  id: 50\n---\n${body}`
				: body;

		it('keeps unresolved refs from the same workspace', async () => {
			const api = createFakeApi();
			await run(api, [
				file('p.md', withFm('acme', '[t](tmgr://task/12) ![f](tmgr://file/7)')),
			]);
			expect(bodyOf(api, 'P')).toBe('[t](tmgr://task/12) ![f](tmgr://file/7)');
		});

		it.each([['other'], [null]])(
			'neutralizes unresolved refs when provenance is %s',
			async (ws) => {
				const api = createFakeApi();
				const result = await run(api, [
					file(
						'p.md',
						withFm(
							ws,
							'[task](tmgr://task/12) ![alt](tmgr://file/7) [d](tmgr://page/acme/x)\n\n[r]: tmgr://task/3\n[ok](https://example.com)',
						),
					),
				]);
				expect(bodyOf(api, ws ? 'P' : 'p')).toBe(
					'task alt d\n\n\n[ok](https://example.com)',
				);
				expect(
					result.warnings.some(
						(w) =>
							/3 tmgr:\/\//.test(w.message) || /4 tmgr:\/\//.test(w.message),
					),
				).toBe(true);
			},
		);

		it('remaps tmgr page ids through provenance of imported files', async () => {
			const api = createFakeApi();
			const a = `---\ntitle: A\ntmgr:\n  workspace: other\n  id: 10\n---\n[to b](tmgr://page/11) [gone](tmgr://page/99)`;
			const b = `---\ntitle: B\ntmgr:\n  workspace: other\n  id: 11\n---\nback [a](tmgr://page/10)`;
			const result = await run(api, [file('a.md', a), file('b.md', b)]);
			const [ida, idb] = result.created.map((c) => c.id);
			expect(bodyOf(api, 'A')).toBe(`[to b](tmgr://page/${idb}) gone`);
			expect(bodyOf(api, 'B')).toBe(`back [a](tmgr://page/${ida})`);
		});

		it('does not leave a foreign tmgr link in the first create call', async () => {
			const api = createFakeApi();
			const create = jest.spyOn(api, 'createPage');
			await run(api, [file('p.md', '[t](tmgr://task/1)')]);
			expect(create.mock.calls[0][0].body).toBe('');
		});
	});

	describe('foreign reference neutralization', () => {
		it('neutralizes autolinks, nested image labels and reference definitions with their uses', async () => {
			const api = createFakeApi();
			const result = await run(api, [
				file(
					'p.md',
					[
						'a <tmgr://task/1> b',
						'[![x](tmgr://file/9)](tmgr://task/123)',
						'[text][r] and [r] and [r][]',
						'',
						'[r]: tmgr://task/3',
					].join('\n'),
				),
			]);
			expect(bodyOf(api, 'p')).toBe(
				['a  b', 'x', 'text and r and r', '', ''].join('\n'),
			);
			expect(result.warnings.some((w) => /tmgr:\/\//.test(w.message))).toBe(
				true,
			);
		});

		it('escapes literal brackets of a neutralized label', async () => {
			const api = createFakeApi();
			await run(api, [file('p.md', '[x [b]](tmgr://task/1)(x.md)')]);
			expect(bodyOf(api, 'p')).toBe('x \\[b\\](x.md)');
		});

		it('keeps an inner mapped image inside a neutralized link label', async () => {
			const api = createFakeApi();
			await run(api, [
				file('p.md', '[![x](pic.png)](tmgr://task/1)'),
				file('pic.png', new Uint8Array([1])),
			]);
			expect(bodyOf(api, 'p')).toMatch(/^!\[x\]\(tmgr:\/\/file\/\d+\)$/);
		});

		it('never treats a local- workspace code as the same workspace', async () => {
			const api = createFakeApi();
			const plan = buildImportPlan(
				[
					file(
						'p.md',
						'---\ntitle: P\ntmgr:\n  workspace: local-1\n  id: 5\n---\n[t](tmgr://task/1)',
					),
				],
				{ existingTitles: [] },
			);
			await runImportWith(api, plan, {
				parentId: null,
				policy: 'rename',
				workspaceCode: 'local-1',
			});
			expect(bodyOf(api, 'P')).toBe('t');
		});
	});

	describe('failures', () => {
		const withLinks = [
			file('A.md', 'see [b](A/B.md) and [t](tmgr://task/1)'),
			file('A/B.md', 'x'),
		];

		it('still finishes the bodies of created pages when a later create fails', async () => {
			const api = createFakeApi();
			api.failCreate = (title) => title === 'B';
			const result = await run(api, withLinks);
			expect(result.error).toMatch(/Could not create "B"/);
			expect(bodyOf(api, 'A')).toBe('see [b](A/B.md) and t');
			expect(result.incomplete).toEqual([]);
			expect(result.warnings.some((w) => /not imported/.test(w.message))).toBe(
				true,
			);
		});

		it('falls back to a neutralized original body when pass 2 fails', async () => {
			const api = createFakeApi();
			api.updateFailures = 1;
			const result = await run(api, [
				file('A.md', 'see [b](B.md) [t](tmgr://task/1)'),
			]);
			expect(result.error).toBeNull();
			expect(result.incomplete).toEqual([]);
			expect(bodyOf(api, 'A')).toBe('see [b](B.md) t');
			expect(
				result.warnings.some((w) => /Could not finish/.test(w.message)),
			).toBe(true);
		});

		it('reports a page whose content could not be saved as incomplete', async () => {
			const api = createFakeApi();
			api.updateFailures = 5;
			const result = await run(api, [file('A.md', '[t](tmgr://task/1)')]);
			expect(result.incomplete).toEqual(['A']);
			expect(bodyOf(api, 'A')).toBe('');
		});

		it('stops with an error and no pass 2 when the workspace changes', async () => {
			const api = createFakeApi();
			api.abortAfter = 1;
			const result = await run(api, [
				file('A.md', '[t](tmgr://task/1)'),
				file('B.md', '[t](tmgr://task/1)'),
			]);
			expect(result.error).toBe('The workspace changed during import');
			expect(result.created).toHaveLength(1);
			expect(api.updates).toEqual([]);
			expect(result.incomplete).toEqual(['A']);
		});

		it('stops during pass 2 when the workspace changes', async () => {
			const api = createFakeApi();
			api.abortAfter = 2;
			const result = await run(api, [
				file('A.md', '[t](tmgr://task/1)'),
				file('B.md', '[t](tmgr://task/2)'),
			]);
			expect(result.error).toBe('The workspace changed during import');
			expect(result.incomplete.sort()).toEqual(['A', 'B']);
		});
	});

	it('does not rename a root to a title another imported root already has', async () => {
		const api = createFakeApi();
		seedPage(api, 'A');
		const result = await run(api, [file('A.md', 'x'), file('A (2).md', 'y')]);
		expect(result.created.map((c) => c.title).sort()).toEqual([
			'A (2)',
			'A (3)',
		]);
	});

	describe('properties', () => {
		const meeting = (ws: string) =>
			`---\ntitle: M\ntype: meeting\nproperties:\n  date: '2026-10-08'\n  participants:\n    - tmgr://page/10\n    - tmgr://user/5\n  related_tasks: [3]\ntmgr:\n  workspace: ${ws}\n  id: 20\n---\nnotes`;
		const person = `---\ntitle: P\ntype: person\nproperties:\n  company: Example Co\n  user_id: 5\ntmgr:\n  workspace: other\n  id: 10\n---\n`;

		it('remaps participants and drops id-bound fields for a foreign workspace', async () => {
			const api = createFakeApi();
			const result = await run(api, [
				file('p.md', person),
				file('m.md', meeting('other')),
			]);
			const ids = Object.fromEntries(
				result.created.map((c) => [c.title, c.id]),
			);
			const props = api.pages.get(ids.M)!.properties;
			expect(props).toEqual({
				date: '2026-10-08',
				participants: [`tmgr://page/${ids.P}`],
				related_tasks: [],
			});
			expect(api.pages.get(ids.P)!.properties).toEqual({
				company: 'Example Co',
			});
			expect(result.warnings.some((w) => /Dropped/.test(w.message))).toBe(true);
		});

		it('keeps id-bound fields for the same workspace', async () => {
			const api = createFakeApi();
			const result = await run(api, [file('m.md', meeting('acme'))]);
			expect(api.pages.get(result.created[0].id)!.properties).toEqual({
				date: '2026-10-08',
				participants: ['tmgr://page/10', 'tmgr://user/5'],
				related_tasks: [3],
			});
		});

		it('retries a create without properties when they are rejected', async () => {
			const api = createFakeApi();
			api.rejectProperties = true;
			const result = await run(api, [file('p.md', person)]);
			expect(result.error).toBeNull();
			expect(result.created).toHaveLength(1);
			expect(result.warnings.some((w) => /rejected/.test(w.message))).toBe(
				true,
			);
		});
	});
});

describe('fixtures', () => {
	it('has text helper', () => {
		expect(text('a')).toEqual(new Uint8Array([97]));
	});
});
