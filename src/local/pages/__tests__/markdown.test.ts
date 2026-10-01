import * as md from '../markdown';

const CONTEXT = md.templateBody('context');
const OPEN_S = '<!-- tmgr:section id="s" owner="persona:p-1" -->';
const OPEN_SYS = '<!-- tmgr:section id="s" owner="system" -->';
const CLOSE = md.SECTION_CLOSE;
const section = (body: string, id: string) => md.findSection(body, id)!;

describe('links and task keys', () => {
	it('extracts tmgr links of every kind once', () => {
		const body =
			'see [A](tmgr://page/12), [T](tmgr://task/7), [C](tmgr://category/3), [U](tmgr://user/9), [P](tmgr://persona/5) and ![](tmgr://file/321) [A again](tmgr://page/12)';
		expect(md.extractLinks(body)).toEqual([
			{ kind: 'page', id: 12 },
			{ kind: 'task', id: 7 },
			{ kind: 'category', id: 3 },
			{ kind: 'user', id: 9 },
			{ kind: 'persona', id: 5 },
		]);
	});

	it('ignores links inside code and comments', () => {
		const body =
			'`[x](tmgr://page/1)`\n\n```\n[y](tmgr://page/2)\n```\n\n<!-- [z](tmgr://page/3) -->\n\n[ok](tmgr://page/4)';
		expect(md.extractLinks(body)).toEqual([{ kind: 'page', id: 4 }]);
	});

	it('finds task key candidates outside code, links and urls', () => {
		const body =
			'Fix TM-212 and AB-1.\n\n`TM-300`\n\n```\nTM-301\n```\n\n[TM-302](tmgr://task/9) [see TM-303](https://x.dev/TM-304) https://x.dev/TM-305 <https://x.dev/TM-306> tm-307 XTM-308x';
		expect(md.taskKeyCandidates(body)).toEqual(['TM-212', 'AB-1']);
		expect(md.taskKeyCandidates('AB-12 then TM-3, AB-12 again, TM-3.')).toEqual(
			['AB-12', 'TM-3'],
		);
	});

	it('autolinks only resolved keys outside protected text, idempotently', () => {
		const body =
			'Fix TM-212 and TM-999.\n\n`TM-212`\n\n[TM-212 done](tmgr://task/1)\n\n```\nTM-212\n```\n';
		const linked = md.autolinkTaskKeys(body, new Map([['TM-212', 456]]));
		expect(linked).toBe(
			'Fix [TM-212](tmgr://task/456) and TM-999.\n\n`TM-212`\n\n[TM-212 done](tmgr://task/1)\n\n```\nTM-212\n```\n',
		);
		expect(md.autolinkTaskKeys(linked, new Map([['TM-212', 456]]))).toBe(
			linked,
		);
	});
});

describe('sections', () => {
	it('parses owner and heading', () => {
		const found = md.sections(CONTEXT);
		expect(found).toHaveLength(1);
		expect([found[0].id, found[0].owner, found[0].heading]).toEqual([
			'agent-notes',
			'agents',
			'Agent notes',
		]);
	});

	it('ignores markers inside fences and unclosed sections', () => {
		expect(
			md.sections(
				'```\n<!-- tmgr:section id="x" owner="agents" -->\n<!-- /tmgr:section -->\n```\n',
			),
		).toEqual([]);
		expect(
			md.sections('<!-- tmgr:section id="x" owner="agents" -->\n## H\n'),
		).toEqual([]);
	});

	it('flags duplicate, nested, unbalanced and stray markers', () => {
		expect(md.structureError(CONTEXT)).toBeNull();
		expect(
			md.structureError(`${OPEN_S}\nx\n${CLOSE}\n${OPEN_SYS}\ny\n${CLOSE}\n`),
		).toContain('more than once');
		expect(
			md.structureError(`${OPEN_S}\n${OPEN_SYS}\n${CLOSE}`),
		).not.toBeNull();
		expect(md.structureError(`${OPEN_S}\nx\n`)).not.toBeNull();
		expect(md.structureError(`x\n${CLOSE}\n`)).not.toBeNull();
		expect(md.structureError(`\`\`\`\n${OPEN_S}\n\`\`\`\n`)).toBeNull();
	});

	it('detects section markers in agent text', () => {
		expect(md.containsSectionMarker('plain <!-- a comment --> text')).toBe(
			false,
		);
		expect(md.containsSectionMarker(`x ${CLOSE}`)).toBe(true);
		expect(
			md.containsSectionMarker('<!--tmgr:section id="a" owner="agents"-->'),
		).toBe(true);
	});
});

describe('append', () => {
	it('adds a paragraph, handles empty bodies and keeps lists together', () => {
		expect(md.appendToEnd('# Title\n\ntext\n', 'new line')).toBe(
			'# Title\n\ntext\n\nnew line\n',
		);
		expect(md.appendToEnd('', 'hello')).toBe('hello\n');
		expect(md.appendToEnd('## Log\n\n- one\n', '- two')).toBe(
			'## Log\n\n- one\n- two\n',
		);
	});

	it('appends under a heading before the next same-level heading', () => {
		expect(
			md.appendUnderHeading(
				'## Timeline\n\n- one\n\n## What I know\n\nfacts\n',
				'timeline',
				'- two',
			),
		).toBe('## Timeline\n\n- one\n- two\n\n## What I know\n\nfacts\n');
		expect(
			md.appendUnderHeading(
				'## A\n\ntext\n\n### Sub\n\nsub text\n\n## B\n',
				'A',
				'tail',
			),
		).toBe('## A\n\ntext\n\n### Sub\n\nsub text\n\ntail\n\n## B\n');
		expect(md.appendUnderHeading('## Notes\n', '## NOTES ', 'x')).toBe(
			'## Notes\n\nx\n',
		);
	});

	it('matches level two only and skips fenced headings', () => {
		expect(
			md.appendUnderHeading('# Notes\n\n### Notes\n', 'Notes', 'x'),
		).toBeNull();
		expect(
			md.appendUnderHeading('```\n## Notes\n```\n', 'Notes', 'x'),
		).toBeNull();
	});

	it('stops at section markers', () => {
		expect(
			md
				.appendUnderHeading(CONTEXT, 'Agent notes', '- item')!
				.endsWith('## Agent notes\n\n- item\n<!-- /tmgr:section -->\n'),
		).toBe(true);
		const body = `## A\n\ntext\n\n${OPEN_SYS}\n## S\n${CLOSE}\n`;
		expect(
			md
				.appendUnderHeading(body, 'A', 'more')!
				.startsWith('## A\n\ntext\n\nmore\n\n<!-- tmgr:section id="s"'),
		).toBe(true);
	});

	it('matches heading aliases in both directions', () => {
		expect(
			md.appendUnderHeading('## Хронология\n\n- one\n', 'Timeline', '- two'),
		).toBe('## Хронология\n\n- one\n- two\n');
		expect(
			md.appendUnderHeading('## Timeline\n\n- one\n', 'хронология', '- two'),
		).toBe('## Timeline\n\n- one\n- two\n');
	});

	it('creates a heading at the end and appends inside a section', () => {
		expect(md.appendNewHeading('# T\n\ntext\n', 'Timeline', '- one')).toBe(
			'# T\n\ntext\n\n## Timeline\n\n- one\n',
		);
		expect(
			md
				.appendInSection(CONTEXT, section(CONTEXT, 'agent-notes'), 'note')
				.endsWith('## Agent notes\n\nnote\n<!-- /tmgr:section -->\n'),
		).toBe(true);
	});

	it('replaces a section keeping its heading unless the text brings one', () => {
		const out = md.replaceSection(
			CONTEXT,
			section(CONTEXT, 'agent-notes'),
			'fresh insight',
		);
		expect(out.startsWith('## How we work\n\n## Architecture\n\n')).toBe(
			true,
		);
		expect(
			out.endsWith(
				'<!-- tmgr:section id="agent-notes" owner="agents" -->\n## Agent notes\n\nfresh insight\n<!-- /tmgr:section -->\n',
			),
		).toBe(true);
		expect(
			md
				.replaceSection(
					CONTEXT,
					section(CONTEXT, 'agent-notes'),
					'## Outcomes\n\nx',
				)
				.endsWith(
					'owner="agents" -->\n## Outcomes\n\nx\n<!-- /tmgr:section -->\n',
				),
		).toBe(true);
	});

	it('counts utf-8 bytes against the limit', () => {
		const ok = 'я'.repeat(md.MAX_BODY_BYTES / 2);
		expect(md.exceedsLimit(ok)).toBe(false);
		expect(md.exceedsLimit(`${ok}я`)).toBe(true);
	});
});

describe('non-human writes', () => {
	const MANAGED = `intro\n\n<!-- tmgr:section id="insights" owner="persona:p-1" -->\n## Insights\n\nold\n${CLOSE}\n\n<!-- tmgr:section id="promises" owner="system" -->\n## Promises\n${CLOSE}\n`;
	const violation = (
		a: string,
		b: string,
		ctx: boolean,
		kind: string,
		ref: string,
	) => md.nonHumanViolation(a, b, ctx, kind, ref);

	it('lets a persona edit the agents section of a context page only', () => {
		const edited = md.appendInSection(
			CONTEXT,
			section(CONTEXT, 'agent-notes'),
			'note',
		);
		expect(violation(CONTEXT, edited, true, 'persona', 'u-1')).toBeNull();
		expect(
			violation(
				CONTEXT,
				CONTEXT.replace('## Architecture', '## Architecture\n\nhijack'),
				true,
				'persona',
				'u-1',
			),
		).not.toBeNull();
		expect(violation('a\n', 'a\nb\n', false, 'persona', 'u-1')).toBeNull();
	});

	it('lets a persona edit its own section but not another persona or system', () => {
		const own = md.replaceSection(MANAGED, section(MANAGED, 'insights'), 'new');
		const sys = md.replaceSection(
			MANAGED,
			section(MANAGED, 'promises'),
			'hack',
		);
		expect(violation(MANAGED, own, false, 'persona', 'p-1')).toBeNull();
		expect(violation(MANAGED, own, false, 'persona', 'p-2')).not.toBeNull();
		expect(violation(MANAGED, sys, false, 'persona', 'p-1')).not.toBeNull();
		expect(violation(MANAGED, own, false, 'plugin', 'p-1')).not.toBeNull();
	});

	it('refuses dropping, reassigning, creating foreign or forging sections', () => {
		expect(
			violation(MANAGED, 'intro\n', false, 'persona', 'p-1'),
		).not.toBeNull();
		expect(
			violation(
				MANAGED,
				MANAGED.replace('persona:p-1', 'agents'),
				false,
				'persona',
				'p-1',
			),
		).not.toBeNull();
		const added = `${MANAGED}\n<!-- tmgr:section id="x" owner="system" -->\n## X\n${CLOSE}\n`;
		expect(violation(MANAGED, added, false, 'persona', 'p-1')).not.toBeNull();
		const before = `intro\n\n${OPEN_S}\n## S\n\nold\n${CLOSE}\n`;
		const forged = before.replace('old', `old\n${CLOSE}\n${OPEN_SYS}\nforged`);
		expect(violation(before, forged, false, 'persona', 'p-1')).not.toBeNull();
	});

	it('writes only agents sections on a context page', () => {
		const body = `## A\n\n${OPEN_S}\n## Mine\n${CLOSE}\n\n<!-- tmgr:section id="agent-notes" owner="agents" -->\n## N\n${CLOSE}\n`;
		const mine = md.replaceSection(body, section(body, 's'), 'x');
		const agents = md.replaceSection(body, section(body, 'agent-notes'), 'x');
		expect(violation(body, mine, true, 'persona', 'p-1')).not.toBeNull();
		expect(violation(body, agents, true, 'persona', 'p-1')).toBeNull();
		expect(violation(body, mine, false, 'persona', 'p-1')).toBeNull();
	});
});

describe('adversarial megabyte inputs', () => {
	it('parse in linear time', () => {
		const inputs = [
			'<!--'.repeat(262_144),
			'['.repeat(1_048_576),
			'`'.repeat(1_048_576),
			'``a'.repeat(349_525),
			'<a:'.repeat(349_525),
			'<!-- tmgr:section id="x'.repeat(43_000),
			`#${' '.repeat(1_048_000)}x`,
			'A'.repeat(1_048_576),
			'![a](b'.repeat(170_000),
			'TM-1 '.repeat(209_715),
		];
		const started = Date.now();
		for (const input of inputs) {
			const t = Date.now();
			md.sections(input);
			md.extractLinks(input);
			md.taskKeyCandidates(input);
			md.headings(input);
			md.structureError(input);
			expect(Date.now() - t).toBeLessThan(1500);
		}
		expect(Date.now() - started).toBeLessThan(10_000);
	});
});
