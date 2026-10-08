import { collectLinks, rewriteForExport, rewriteMarkdownLinks } from '../links';

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
