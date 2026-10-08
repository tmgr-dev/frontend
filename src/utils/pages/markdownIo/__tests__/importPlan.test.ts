import { buildImportPlan } from '../importPlan';
import { IMPORT_LIMITS } from '../types';
import { file } from './fakeApi';

const plan = (files: ReturnType<typeof file>[], existing: string[] = []) =>
	buildImportPlan(files, { existingTitles: existing });

const shape = (p: ReturnType<typeof plan>) =>
	p.pages.map((page) => [
		page.title,
		p.pages.find((x) => x.key === page.parentKey)?.title ?? null,
	]);

describe('buildImportPlan tree mapping', () => {
	it('maps X.md plus X/ to a parent and children', () => {
		const p = plan([
			file('Guide.md', 'root'),
			file('Guide/One.md', 'a'),
			file('Guide/Two.md', 'b'),
			file('Guide/Deep/Three.md', 'c'),
		]);
		expect(shape(p)).toEqual([
			['Guide', null],
			['Deep', 'Guide'],
			['Three', 'Deep'],
			['One', 'Guide'],
			['Two', 'Guide'],
		]);
		expect(p.pages.find((x) => x.title === 'Deep')?.path).toBeNull();
	});

	it('uses index.md and README.md as the folder page', () => {
		const p = plan([
			file('Docs/index.md', '# Docs home\n\nhello'),
			file('Docs/Page.md', 'p'),
			file('Wiki/README.md', 'readme body'),
			file('Wiki/Other.md', 'o'),
		]);
		expect(shape(p)).toEqual([
			['Docs home', null],
			['Page', 'Docs home'],
			['Wiki', null],
			['Other', 'Wiki'],
		]);
		expect(p.pages[0].body).toBe('hello');
		expect(p.pages.find((x) => x.title === 'Wiki')?.body).toBe('readme body');
	});

	it('creates an empty plain page for a folder without a page and ignores asset folders', () => {
		const p = plan([
			file('Folder/Note.md', 'n'),
			file('images/pic.png', new Uint8Array([1])),
		]);
		expect(shape(p)).toEqual([
			['Folder', null],
			['Note', 'Folder'],
		]);
		expect(p.pages[0]).toMatchObject({ body: '', type: 'plain', path: null });
	});

	it('strips Notion id suffixes from titles', () => {
		const id = 'a'.repeat(32);
		const p = plan([
			file(`Project ${id}.md`, 'x'),
			file(`Project ${id}/Task ${id}.md`, 'y'),
		]);
		expect(shape(p)).toEqual([
			['Project', null],
			['Task', 'Project'],
		]);
	});

	it('keeps duplicate names in different folders and orders parents first', () => {
		const p = plan([
			file('A/Same.md', '1'),
			file('B/Same.md', '2'),
			file('Same.md', '3'),
		]);
		expect(p.pages.map((x) => x.title)).toEqual([
			'A',
			'Same',
			'B',
			'Same',
			'Same',
		]);
		expect(new Set(p.pages.map((x) => x.key)).size).toBe(5);
		const seen = new Set<string>();
		for (const page of p.pages) {
			if (page.parentKey) expect(seen.has(page.parentKey)).toBe(true);
			seen.add(page.key);
		}
	});
});

describe('buildImportPlan content', () => {
	it('resolves title from frontmatter, then H1, then file name', () => {
		const p = plan([
			file('a.md', '---\ntitle: From FM\n---\n# Heading\nbody'),
			file('b.md', '\n# From H1 #\n\nbody b'),
			file('c.md', 'no heading'),
			file('d.md', 'text first\n\n# Later heading'),
		]);
		const by = Object.fromEntries(p.pages.map((x) => [x.path, x]));
		expect(by['a.md']).toMatchObject({
			title: 'From FM',
			body: '# Heading\nbody',
		});
		expect(by['b.md']).toMatchObject({ title: 'From H1', body: 'body b' });
		expect(by['c.md'].title).toBe('c');
		expect(by['d.md'].title).toBe('d');
	});

	it('maps types, warns for context and keeps properties only on typed pages', () => {
		const p = plan([
			file('ctx.md', '---\ntype: context\n---\nx'),
			file('note.md', '---\ntype: note\nproperties:\n  company: X\n---\nx'),
			file(
				'who.md',
				'---\ntype: person\nproperties:\n  company: Example Co\ntmgr:\n  workspace: acme\n  id: 4\n---\nx',
			),
		]);
		const by = Object.fromEntries(p.pages.map((x) => [x.path, x]));
		expect(by['ctx.md'].type).toBe('plain');
		expect(by['note.md']).toMatchObject({ type: 'plain', properties: null });
		expect(by['who.md']).toMatchObject({
			type: 'person',
			properties: { company: 'Example Co' },
			sourceId: 4,
			sourceWorkspace: 'acme',
		});
		expect(p.warnings.filter((w) => /context/.test(w.message))).toHaveLength(1);
	});

	it('lists ignored frontmatter keys once', () => {
		const p = plan([
			file('a.md', '---\ntags: [x]\n---\nA'),
			file('b.md', '---\ntags: [y]\ncreated: 2026-01-01\n---\nB'),
		]);
		const ignored = p.warnings.filter((w) =>
			/Ignored frontmatter keys/.test(w.message),
		);
		expect(ignored).toHaveLength(1);
		expect(ignored[0].message).toBe('Ignored frontmatter keys: created, tags');
	});

	it('skips oversized bodies and re-parents their children', () => {
		const p = buildImportPlan(
			[file('Big.md', 'x'.repeat(30)), file('Big/Child.md', 'c')],
			{ existingTitles: [] },
			{ ...IMPORT_LIMITS, maxBodyBytes: 10 },
		);
		expect(shape(p)).toEqual([['Child', null]]);
		expect(
			p.warnings.some((w) => w.path === 'Big.md' && /Skipped/.test(w.message)),
		).toBe(true);
	});

	it('warns about broken relative links and unreferenced files, keeps referenced ones', () => {
		const p = plan([
			file(
				'a.md',
				'[ok](b.md) [gone](missing.md) ![i](img/p%20q.png) [[Nope]] [web](https://example.com/x.md)',
			),
			file('b.md', 'b'),
			file('img/p q.png', new Uint8Array([1])),
			file('img/unused.png', new Uint8Array([2])),
		]);
		const messages = p.warnings.map((w) => w.message);
		expect(messages).toContain('Link target not found: missing.md');
		expect(messages).toContain('Link target not found: Nope');
		expect(messages.filter((m) => /Link target/.test(m))).toHaveLength(2);
		expect(p.warnings.find((w) => w.path === 'img/unused.png')).toBeTruthy();
		expect(Object.keys(p.files)).toEqual(['img/p q.png']);
	});

	it('flags top-level conflicts case-insensitively against existing children', () => {
		const p = plan(
			[
				file('Guide.md', 'x'),
				file('Guide/Inner.md', 'y'),
				file('Fresh.md', 'z'),
			],
			[' guide '],
		);
		expect(
			Object.fromEntries(p.pages.map((x) => [x.title, x.conflict])),
		).toEqual({
			Guide: true,
			Inner: false,
			Fresh: false,
		});
	});

	it('warns when there is nothing to import', () => {
		expect(
			plan([file('a.png', new Uint8Array([1]))]).warnings[0].message,
		).toMatch(/No Markdown/);
	});
});
