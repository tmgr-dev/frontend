import {
	applyFreeEdit,
	joinSegments,
	padSegments,
	parseOwner,
	replaceSectionInner,
	sectionInner,
	splitBody,
	type Segment,
} from '../sections';

const CONTEXT = `## How we work

## Architecture

<!-- tmgr:section id="agent-notes" owner="agents" -->
## Agent notes
<!-- /tmgr:section -->
`;

const kinds = (segments: Segment[]) => segments.map((s) => s.kind);

describe('splitBody', () => {
	it('keeps a body without markers as one free segment', () => {
		expect(splitBody('## A\n\ntext\n')).toEqual([
			{ kind: 'free', text: '## A\n\ntext\n' },
		]);
	});

	it('returns no segments for an empty body', () => {
		expect(splitBody('')).toEqual([]);
	});

	it('splits the context template into free, section', () => {
		const segments = splitBody(CONTEXT);
		expect(kinds(segments)).toEqual(['free', 'section']);
		const section = segments[1] as Extract<Segment, { kind: 'section' }>;
		expect(section.id).toBe('agent-notes');
		expect(section.owner).toBe('agents');
		expect(section.inner).toBe('## Agent notes\n');
		expect(section.heading).toBe('Agent notes');
	});

	it('treats an unclosed marker as free text', () => {
		const body = 'a\n<!-- tmgr:section id="x" owner="agents" -->\nb\n';
		expect(splitBody(body)).toEqual([{ kind: 'free', text: body }]);
	});

	it('ignores markers inside a code fence', () => {
		const body =
			'```\n<!-- tmgr:section id="x" owner="agents" -->\nb\n<!-- /tmgr:section -->\n```\n';
		expect(splitBody(body)).toEqual([{ kind: 'free', text: body }]);
	});

	it('finds two adjacent sections', () => {
		const body =
			'<!-- tmgr:section id="a" owner="system" -->\none\n<!-- /tmgr:section -->\n<!-- tmgr:section id="b" owner="persona:5" -->\ntwo\n<!-- /tmgr:section -->\n';
		const segments = splitBody(body);
		expect(kinds(segments)).toEqual(['section', 'section']);
	});

	it('handles a section closed at EOF without trailing newline', () => {
		const body =
			'x\n<!-- tmgr:section id="a" owner="agents" -->\nbody\n<!-- /tmgr:section -->';
		const segments = splitBody(body);
		expect(kinds(segments)).toEqual(['free', 'section']);
		expect(joinSegments(segments)).toBe(body);
	});

	it('does not nest: an inner open marker belongs to the section content', () => {
		const body =
			'<!-- tmgr:section id="a" owner="agents" -->\n<!-- tmgr:section id="b" owner="agents" -->\n<!-- /tmgr:section -->\n';
		const segments = splitBody(body);
		expect(kinds(segments)).toEqual(['section']);
	});
});

describe('joinSegments', () => {
	const samples: Record<string, string> = {
		template: CONTEXT,
		crlf: '## A\r\n\r\n<!-- tmgr:section id="x" owner="system" -->\r\nbody\r\n<!-- /tmgr:section -->\r\ntail\r\n',
		noTrailingNewline:
			'text\n<!-- tmgr:section id="x" owner="agents" -->\nb\n<!-- /tmgr:section -->\ntail',
		adjacent:
			'<!-- tmgr:section id="a" owner="system" -->\n1\n<!-- /tmgr:section -->\n<!-- tmgr:section id="b" owner="agents" -->\n2\n<!-- /tmgr:section -->\n',
		unclosed: 'a\n<!-- tmgr:section id="x" owner="agents" -->\nb',
		fenced: '```\n<!-- tmgr:section id="x" owner="agents" -->\n```\n',
		spacing:
			'<!--   tmgr:section   id="x"   owner="agents"   -->\nb\n<!-- /tmgr:section -->\n',
		empty: '',
		onlyBlank: '\n\n',
	};

	it.each(Object.entries(samples))(
		'round-trips %s byte for byte',
		(_n, body) => {
			expect(joinSegments(splitBody(body))).toBe(body);
			expect(joinSegments(padSegments(splitBody(body)))).toBe(body);
		},
	);
});

describe('padSegments', () => {
	it('adds an editable free segment after the last section', () => {
		const padded = padSegments(splitBody(CONTEXT));
		expect(kinds(padded)).toEqual(['free', 'section', 'free']);
		expect((padded[2] as { text: string }).text).toBe('');
	});

	it('adds free segments around and between adjacent sections', () => {
		const body =
			'<!-- tmgr:section id="a" owner="system" -->\n1\n<!-- /tmgr:section -->\n<!-- tmgr:section id="b" owner="agents" -->\n2\n<!-- /tmgr:section -->\n';
		expect(kinds(padSegments(splitBody(body)))).toEqual([
			'free',
			'section',
			'free',
			'section',
			'free',
		]);
	});

	it('gives an empty body one free segment', () => {
		expect(padSegments([])).toEqual([{ kind: 'free', text: '' }]);
	});
});

describe('applyFreeEdit', () => {
	it('keeps the marker on its own line after the edited text', () => {
		const padded = padSegments(splitBody(CONTEXT));
		const edited = applyFreeEdit(padded, 0, '## How we work\n\nnew text');
		const joined = joinSegments(edited);
		expect(joined).toContain('new text\n\n<!-- tmgr:section id="agent-notes"');
		expect(joined.endsWith('<!-- /tmgr:section -->\n')).toBe(true);
	});

	it('appends text after the last section', () => {
		const padded = padSegments(splitBody(CONTEXT));
		const joined = joinSegments(applyFreeEdit(padded, 2, 'tail'));
		expect(joined).toBe(`${CONTEXT}tail\n`);
	});

	it('does not touch other segments', () => {
		const padded = padSegments(splitBody(CONTEXT));
		const edited = applyFreeEdit(padded, 2, 'tail');
		expect(edited[0]).toBe(padded[0]);
		expect(edited[1]).toBe(padded[1]);
	});

	it('an emptied free segment before a section collapses to nothing', () => {
		const padded = padSegments(splitBody(CONTEXT));
		const joined = joinSegments(applyFreeEdit(padded, 0, ''));
		expect(joined.startsWith('<!-- tmgr:section')).toBe(true);
	});

	it('does not mutate the input', () => {
		const padded = padSegments(splitBody(CONTEXT));
		const copy = JSON.parse(JSON.stringify(padded));
		applyFreeEdit(padded, 0, 'x');
		expect(padded).toEqual(copy);
	});
});

describe('parseOwner', () => {
	it.each([
		['system', { kind: 'system', ref: null }],
		['agents', { kind: 'agents', ref: null }],
		['persona:5', { kind: 'persona', ref: '5' }],
		['plugin:tmgr.people', { kind: 'plugin', ref: 'tmgr.people' }],
		['user:9', { kind: 'user', ref: '9' }],
		['weird', { kind: 'unknown', ref: null }],
	])('%s', (owner, expected) => {
		expect(parseOwner(owner)).toEqual(expected);
	});
});

describe('replaceSectionInner / sectionInner', () => {
	it('replaces only the named section content', () => {
		const next = replaceSectionInner(
			CONTEXT,
			'agent-notes',
			'## Agent notes\n- new\n',
		);
		expect(next).toBe(
			CONTEXT.replace('## Agent notes\n', '## Agent notes\n- new\n'),
		);
	});

	it('leaves the body alone for an unknown section', () => {
		expect(replaceSectionInner(CONTEXT, 'nope', 'x')).toBe(CONTEXT);
	});

	it('reads the inner text of a section', () => {
		expect(sectionInner(CONTEXT, 'agent-notes')).toBe('## Agent notes\n');
		expect(sectionInner(CONTEXT, 'nope')).toBeNull();
	});
});
