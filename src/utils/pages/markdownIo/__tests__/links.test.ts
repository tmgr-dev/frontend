import {
	collectLinks,
	escapeBrackets,
	rewriteForExport,
	rewriteMarkdownLinks,
} from '../links';

const ctx = (extra: Partial<Parameters<typeof rewriteForExport>[1]> = {}) => ({
	fromPath: 'Root.md',
	workspaceCode: 'acme',
	pagePaths: new Map([
		[2, 'Root/Child page.md'],
		[1, 'Root.md'],
	]),
	pageSlugPaths: new Map([['child-page', 'Root/Child page.md']]),
	filePaths: new Map([[9, 'assets/9-pic (1).png']]),
	...extra,
});

describe('rewriteMarkdownLinks', () => {
	it('visits links, images, wikilinks, embeds and definitions', () => {
		const kinds = collectLinks(
			'[a](x.md) ![b](y.png) [[Z]] ![[w.png]]\n\n[ref]: q.md\n',
		).map((t) => [t.kind, t.target]);
		expect(kinds).toEqual([
			['link', 'x.md'],
			['image', 'y.png'],
			['wikilink', 'Z'],
			['embed', 'w.png'],
			['definition', 'q.md'],
		]);
	});

	it('leaves fenced code and inline code untouched', () => {
		const md = [
			'`[a](x.md)` and [b](x.md)',
			'```',
			'[c](x.md)',
			'```',
			'~~~md',
			'[d](x.md)',
			'~~~',
			'[e](x.md)',
		].join('\n');
		const out = rewriteMarkdownLinks(md, (t) => t.build('Z'));
		expect(out).toBe(
			[
				'`[a](x.md)` and [b](Z)',
				'```',
				'[c](x.md)',
				'```',
				'~~~md',
				'[d](x.md)',
				'~~~',
				'[e](Z)',
			].join('\n'),
		);
	});

	it('keeps titles and wraps urls with spaces', () => {
		const out = rewriteMarkdownLinks('[a](x.md "T")', (t) => t.build('a b.md'));
		expect(out).toBe('[a](<a b.md> "T")');
	});
});

describe('rewriteForExport', () => {
	it('maps page links by id and by deep slug to encoded relative paths', () => {
		const out = rewriteForExport(
			'[c](tmgr://page/2#intro) [d](tmgr://page/acme/child-page) [o](tmgr://page/acme/other) [x](tmgr://page/99)',
			ctx(),
		);
		expect(out).toBe(
			'[c](Root/Child%20page.md#intro) [d](Root/Child%20page.md) [o](tmgr://page/acme/other) [x](tmgr://page/99)',
		);
	});

	it('does not rewrite deep links from another workspace', () => {
		const out = rewriteForExport('[d](tmgr://page/other/child-page)', ctx());
		expect(out).toBe('[d](tmgr://page/other/child-page)');
	});

	it('maps files relative to the page and encodes parentheses', () => {
		const out = rewriteForExport(
			'![p](tmgr://file/9) [u](tmgr://file/10)',
			ctx({ fromPath: 'Root/Child page.md' }),
		);
		expect(out).toBe('![p](../assets/9-pic%20%281%29.png) [u](tmgr://file/10)');
	});

	it('ignores code and non-page tmgr links', () => {
		const md =
			'`[c](tmgr://page/2)`\n```\n[c](tmgr://page/2)\n```\n[t](tmgr://task/5)';
		expect(rewriteForExport(md, ctx())).toBe(md);
	});
});

describe('nested and block constructs', () => {
	it('visits an image inside a link label and rewrites both', () => {
		const md = '[![alt](a.png)](b.md)';
		expect(collectLinks(md).map((t) => [t.kind, t.target])).toEqual([
			['image', 'a.png'],
			['link', 'b.md'],
		]);
		expect(rewriteMarkdownLinks(md, (t) => t.build(`Z/${t.target}`))).toBe(
			'[![alt](Z/a.png)](Z/b.md)',
		);
	});

	it('keeps the rewritten label when only the inner token changes', () => {
		const out = rewriteMarkdownLinks('[![alt](a.png)](b.md "t")', (t) =>
			t.kind === 'image' ? t.build('Z.png') : null,
		);
		expect(out).toBe('[![alt](Z.png)](b.md "t")');
	});

	it('collects autolinks and ignores bare angle text', () => {
		expect(
			collectLinks('<tmgr://task/1> <b>x</b> <https://a.b/c>').map((t) => [
				t.kind,
				t.target,
			]),
		).toEqual([
			['autolink', 'tmgr://task/1'],
			['autolink', 'https://a.b/c'],
		]);
	});

	it('honours escaped brackets', () => {
		expect(
			collectLinks('\\[a](x.md) [b\\]](y.md)').map((t) => t.target),
		).toEqual(['y.md']);
	});

	it('does not pair backticks across a paragraph break', () => {
		const md = 'a ` b\n\n[x](t.md) ` c';
		expect(collectLinks(md).map((t) => t.target)).toEqual(['t.md']);
	});

	it('treats indented code after a blank line or at the start as code', () => {
		expect(
			collectLinks('    [a](x.md)\n\ntext\n\n    [b](y.md)\n\n\t[c](z.md)'),
		).toEqual([]);
		expect(collectLinks('text\n    [a](x.md)').map((t) => t.target)).toEqual([
			'x.md',
		]);
		expect(
			collectLinks('- item\n\n    [a](x.md)').map((t) => t.target),
		).toEqual(['x.md']);
	});

	it('matches reference definitions only at a real line start', () => {
		expect(collectLinks('`code` [r]: tmgr://task/1')).toEqual([]);
		expect(collectLinks('a\n[r]: tmgr://task/1').map((t) => t.kind)).toEqual([
			'definition',
		]);
		expect(collectLinks('[^1]: footnote text')).toEqual([]);
	});

	it('resolves reference uses only for requested labels', () => {
		const md =
			'[text][r] and [r] and [r][] and [other][o]\n\n[r]: tmgr://task/1';
		const out = rewriteMarkdownLinks(
			md,
			(t) => (t.kind === 'reference' ? t.plain : null),
			{ references: new Set(['r']) },
		);
		expect(out).toBe('text and r and r and [other][o]\n\n[r]: tmgr://task/1');
	});

	it('escapes only literal brackets in the plain label', () => {
		let plain = '';
		rewriteMarkdownLinks('[a [b] ![i](p.png) \\[](x)', (t) => {
			if (t.kind === 'link') plain = t.plain;
			return null;
		});
		expect(plain).toBe('a \\[b\\] ![i](p.png) \\[');
		expect(escapeBrackets('x\\[y [z]')).toBe('x\\[y \\[z\\]');
	});
});

describe('export of definitions and nested images', () => {
	it('rewrites reference definitions like inline links', () => {
		const out = rewriteForExport(
			'![i][f]\n\n[f]: tmgr://file/9\n[p]: tmgr://page/2#x',
			ctx(),
		);
		expect(out).toBe(
			'![i][f]\n\n[f]: assets/9-pic%20%281%29.png\n[p]: Root/Child%20page.md#x',
		);
	});

	it('rewrites an image nested in a link label', () => {
		expect(
			rewriteForExport('[![x](tmgr://file/9)](tmgr://page/2)', ctx()),
		).toBe('[![x](assets/9-pic%20%281%29.png)](Root/Child%20page.md)');
	});
});

describe('linear time', () => {
	const MB = 1_000_000;
	const inputs: [string, string][] = [
		['open brackets', '['.repeat(MB)],
		['link starts', '[a]('.repeat(MB / 4)],
		['backticks', '`'.repeat(MB)],
		['backtick pairs', '` `` ```'.repeat(MB / 8)],
		['wikilink starts', '[['.repeat(MB / 2)],
		['image starts', '![a]('.repeat(MB / 5)],
		['angle brackets', '<a:'.repeat(MB / 3)],
		['close brackets', ']('.repeat(MB / 2)],
	];
	it.each(inputs)('handles 1 MB of %s quickly', (_name, text) => {
		const started = Date.now();
		collectLinks(text);
		rewriteMarkdownLinks(text, () => null);
		rewriteMarkdownLinks(text, (t) => t.plain, { references: new Set(['a']) });
		expect(Date.now() - started).toBeLessThan(2000);
	});
});
