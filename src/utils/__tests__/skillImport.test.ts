import {
	byteLength,
	parseSkillFrontMatter,
	SKILL_BODY_MAX_BYTES,
	validateSkillMarkdown,
} from '../personas';
import {
	convertSkill,
	defaultTicked,
	importSummary,
	indexStorageKey,
	loadIndex,
	planImport,
	recordImport,
	rowStatus,
	saveIndex,
	slugify,
	suggestSlug,
	type IndexEntry,
	type KeyValueStorage,
	type ScanSkill,
} from '../skillImport';

const makeSkill = (overrides: Partial<ScanSkill> = {}): ScanSkill => {
	const content = overrides.content ?? '';
	return {
		key: 'claude:personal:triage',
		dir_name: 'triage',
		path: '~/.claude/skills/triage/SKILL.md',
		content,
		hash: 'hash-1',
		size: byteLength(content),
		too_large: false,
		secret: false,
		secret_reason: null,
		symlink_target: null,
		supporting: [],
		supporting_more: 0,
		...overrides,
	};
};

const lines = (...rows: string[]) => rows.join('\n');

const memoryStorage = (initial: Record<string, string> = {}) => {
	const data: Record<string, string> = { ...initial };
	const storage: KeyValueStorage = {
		getItem: (key) => (key in data ? data[key] : null),
		setItem: (key, value) => {
			data[key] = value;
		},
	};
	return { storage, data };
};

const ENTRY: IndexEntry = {
	slug: 'triage',
	sourceHash: 'hash-1',
	libraryVersion: 3,
	importedAt: '2026-11-01T10:00:00Z',
};

describe('slugify', () => {
	it('lowercases and turns separators into single dashes', () => {
		expect(slugify('Triage Tool', 'x', 'k')).toBe('triage-tool');
		expect(slugify('my__skill..v2', 'x', 'k')).toBe('my-skill-v2');
	});

	it('folds accents to ASCII', () => {
		expect(slugify('Café Résumé', 'x', 'k')).toBe('cafe-resume');
	});

	it('drops symbols and trims dashes', () => {
		expect(slugify('--C++ / Go!!--', 'x', 'k')).toBe('c-go');
	});

	it('falls back to the dir name when the name has no ASCII letters', () => {
		expect(slugify('Триаж', 'triage-dir', 'k')).toBe('triage-dir');
	});

	it('falls back to a deterministic hash when neither name has ASCII letters', () => {
		const first = slugify('Триаж', 'почта', 'claude:personal:почта');
		expect(first).toMatch(/^skill-[0-9a-f]{6}$/);
		expect(slugify('Триаж', 'почта', 'claude:personal:почта')).toBe(first);
		expect(slugify('Триаж', 'почта', 'other-seed')).not.toBe(first);
	});

	it('caps the slug at 60 characters', () => {
		expect(slugify('a'.repeat(80), 'x', 'k')).toHaveLength(60);
	});

	it('does not end the capped slug with a dash', () => {
		expect(slugify(`${'a'.repeat(59)}-b`, 'x', 'k')).toBe('a'.repeat(59));
	});
});

describe('suggestSlug', () => {
	it('prefixes the slug with the package name', () => {
		expect(suggestSlug('Superpowers Plugin', 'brainstorm')).toBe(
			'superpowers-plugin-brainstorm',
		);
	});
});

describe('convertSkill front matter', () => {
	it('takes slug from name, when from description, title from the H1', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: Triage Inbox',
					'description: Sort the inbox every morning',
					'---',
					'# Inbox Triage',
					'',
					'Steps follow.',
				),
			}),
		);
		expect(converted.slug).toBe('triage-inbox');
		expect(converted.when).toBe('Sort the inbox every morning');
		expect(converted.title).toBe('Inbox Triage');
		expect(converted.blockedReason).toBeNull();
		expect(converted.markdown).toBe(
			lines(
				'---',
				'slug: triage-inbox',
				'title: Inbox Triage',
				'when: Sort the inbox every morning',
				'actions: []',
				'---',
				'# Inbox Triage',
				'',
				'Steps follow.',
			),
		);
	});

	it('lets slugOverride win over the name', () => {
		const converted = convertSkill(
			makeSkill({
				content: '---\nname: Triage\ndescription: d\n---\nbody\n',
			}),
			'my-triage',
		);
		expect(converted.slug).toBe('my-triage');
		expect(converted.markdown).toContain('slug: my-triage\n');
	});

	it('reads a quoted description containing ": "', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: triage',
					'description: "Use for triage: sorting the inbox"',
					'---',
					'body',
				),
			}),
		);
		expect(converted.when).toBe('Use for triage: sorting the inbox');
	});

	it('reads a folded (>) description as one line', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: triage',
					'description: >',
					'  Sort the inbox',
					'  every morning',
					'---',
					'body',
				),
			}),
		);
		expect(converted.when).toBe('Sort the inbox every morning');
	});

	it('reads a literal (|) description as one line', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: triage',
					'description: |',
					'  Sort the inbox',
					'  every morning',
					'---',
					'body',
				),
			}),
		);
		expect(converted.when).toBe('Sort the inbox every morning');
	});

	it('reads a stripped folded (>-) description as one line', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: triage',
					'description: >-',
					'  Sort the inbox',
					'  every morning',
					'---',
					'body',
				),
			}),
		);
		expect(converted.when).toBe('Sort the inbox every morning');
	});

	it('prefixes a when that would parse as a list or delimiter', () => {
		const bracket = convertSkill(
			makeSkill({
				content: '---\nname: triage\ndescription: "[beta] triage"\n---\nbody\n',
			}),
		);
		expect(bracket.when).toBe('Use when: [beta] triage');
		expect(validateSkillMarkdown(bracket.markdown, bracket.slug)).toBeNull();

		const delimiter = convertSkill(
			makeSkill({
				content: '---\nname: triage\ndescription: "---"\n---\nbody\n',
			}),
		);
		expect(delimiter.when).toBe('Use when: ---');
	});

	it('cuts an over-long description to 1024 characters with a warning', () => {
		const converted = convertSkill(
			makeSkill({
				content: `---\nname: triage\ndescription: ${'word '.repeat(400)}\n---\nbody\n`,
			}),
		);
		expect(converted.when.length).toBeLessThanOrEqual(1024);
		expect(converted.warnings).toContain('description cut to 1024 characters');
		expect(converted.blockedReason).toBeNull();
	});

	it('lists dropped front matter keys and keeps the body verbatim', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: triage',
					'description: d',
					'allowed-tools: Read, Grep',
					'model: opus',
					'---',
					'',
					'  indented   line  ',
				),
			}),
		);
		expect(converted.droppedFields).toEqual(['allowed-tools', 'model']);
		expect(converted.markdown.endsWith('---\n  indented   line  ')).toBe(true);
	});

	it('warns and falls back to the dir name when there is no front matter', () => {
		const converted = convertSkill(
			makeSkill({
				dir_name: 'code-review',
				content: '# Review code\n\nLook at the diff first.\n',
			}),
		);
		expect(converted.slug).toBe('code-review');
		expect(converted.when).toBe('Look at the diff first.');
		expect(converted.warnings).toContain('no front matter');
		expect(converted.blockedReason).toBeNull();
	});

	it('takes when from the first body paragraph when there is no description', () => {
		const converted = convertSkill(
			makeSkill({
				content:
					'---\nname: triage\n---\n# Title\n\nFirst paragraph\nhere.\n\nSecond.\n',
			}),
		);
		expect(converted.when).toBe('First paragraph here.');
	});

	it('blocks a skill with no description and no body paragraph', () => {
		const converted = convertSkill(
			makeSkill({ content: '---\nname: triage\n---\n# Only a heading\n' }),
		);
		expect(converted.blockedReason).toBe('no description');
	});

	it('flags an invalid YAML front matter and falls back to the body', () => {
		const converted = convertSkill(
			makeSkill({
				dir_name: 'broken',
				content: '---\nname: [unclosed\n---\nBody paragraph.\n',
			}),
		);
		expect(converted.slug).toBe('broken');
		expect(converted.when).toBe('Body paragraph.');
		expect(converted.warnings.join(' ')).toMatch(/not valid YAML/);
	});

	it('blocks a YAML alias bomb instead of expanding it', () => {
		const bomb = [
			'---',
			'name: triage',
			'description: &a [x, x, x, x, x, x, x, x, x, x]',
			'x1: &b [*a, *a, *a, *a, *a, *a, *a, *a, *a, *a]',
			'x2: &c [*b, *b, *b, *b, *b, *b, *b, *b, *b, *b]',
			'x3: &d [*c, *c, *c, *c, *c, *c, *c, *c, *c, *c]',
			'x4: &e [*d, *d, *d, *d, *d, *d, *d, *d, *d, *d]',
			'x5: &f [*e, *e, *e, *e, *e, *e, *e, *e, *e, *e]',
			'x6: &g [*f, *f, *f, *f, *f, *f, *f, *f, *f, *f]',
			'---',
			'Body paragraph.',
		].join('\n');
		const converted = convertSkill(makeSkill({ content: bomb }));
		expect(converted.warnings.join(' ')).toMatch(/not valid YAML/);
	});
});

describe('convertSkill title', () => {
	it('ignores a # line inside a code fence', () => {
		const converted = convertSkill(
			makeSkill({
				content: lines(
					'---',
					'name: triage',
					'description: d',
					'---',
					'```bash',
					'# not a title',
					'```',
					'# Real Title',
				),
			}),
		);
		expect(converted.title).toBe('Real Title');
	});

	it('strips emphasis, backticks and links from the H1', () => {
		const converted = convertSkill(
			makeSkill({
				content:
					'---\nname: triage\ndescription: d\n---\n# **Bold** `code` [link](http://x.test)\n',
			}),
		);
		expect(converted.title).toBe('Bold code link');
	});

	it('humanises the name when there is no H1', () => {
		const converted = convertSkill(
			makeSkill({
				content: '---\nname: inbox_zero-helper\ndescription: d\n---\nbody\n',
			}),
		);
		expect(converted.title).toBe('Inbox zero helper');
	});

	it('caps the title at 120 characters', () => {
		const converted = convertSkill(
			makeSkill({
				content: `---\nname: triage\ndescription: d\n---\n# ${'x'.repeat(200)}\n`,
			}),
		);
		expect(converted.title).toHaveLength(120);
	});
});

describe('convertSkill warnings', () => {
	it('counts listed and unlisted supporting files', () => {
		const converted = convertSkill(
			makeSkill({
				content: '---\nname: triage\ndescription: d\n---\nbody\n',
				supporting: [{ name: 'helper.md', size: 10 }],
				supporting_more: 2,
			}),
		);
		expect(converted.warnings).toContain(
			'3 supporting files not imported; relative references will not resolve',
		);
	});

	it('warns on the CLAUDE_PLUGIN_ROOT variable', () => {
		const converted = convertSkill(
			makeSkill({
				content:
					'---\nname: triage\ndescription: d\n---\nRun ${CLAUDE_PLUGIN_ROOT}/x.sh\n',
			}),
		);
		expect(converted.warnings.join(' ')).toMatch(/CLAUDE_PLUGIN_ROOT/);
	});

	it('warns on relative links and scripts paths', () => {
		const link = convertSkill(
			makeSkill({
				content:
					'---\nname: triage\ndescription: d\n---\nSee [guide](./guide.md)\n',
			}),
		);
		const script = convertSkill(
			makeSkill({
				content:
					'---\nname: triage\ndescription: d\n---\nRun scripts/check.sh\n',
			}),
		);
		expect(link.warnings.join(' ')).toMatch(/relative paths/);
		expect(script.warnings.join(' ')).toMatch(/relative paths/);
	});

	it('does not warn on a plugin:skill style reference', () => {
		const converted = convertSkill(
			makeSkill({
				content:
					'---\nname: triage\ndescription: d\n---\nUse superpowers:brainstorming first.\n',
			}),
		);
		expect(converted.warnings).toEqual([]);
	});
});

describe('convertSkill blocking', () => {
	const VALID = '---\nname: triage\ndescription: d\n---\nbody\n';

	it('blocks a secret-flagged skill', () => {
		const converted = convertSkill(
			makeSkill({ content: '', secret: true, secret_reason: 'AKIA' }),
		);
		expect(converted.blockedReason).toBe('looks like it contains a secret');
	});

	it('blocks a too-large skill with its size', () => {
		const converted = convertSkill(
			makeSkill({ content: '', too_large: true, size: 20 * 1024 + 1 }),
		);
		expect(converted.blockedReason).toBe('21 KiB, limit 16 KiB');
	});

	it('blocks an empty file', () => {
		expect(convertSkill(makeSkill({ content: '   \n' })).blockedReason).toBe(
			'empty file',
		);
	});

	it('allows a converted body of exactly 16 KiB', () => {
		const header = convertSkill(
			makeSkill({ content: '---\nname: big\ndescription: d\n---\n' }),
		).markdown;
		const padding = 'a'.repeat(SKILL_BODY_MAX_BYTES - byteLength(header));
		const converted = convertSkill(
			makeSkill({
				dir_name: 'big',
				content: `---\nname: big\ndescription: d\n---\n${padding}`,
			}),
		);
		expect(converted.bytes).toBe(SKILL_BODY_MAX_BYTES);
		expect(converted.blockedReason).toBeNull();
	});

	it('blocks a converted body one byte over 16 KiB', () => {
		const header = convertSkill(
			makeSkill({ content: '---\nname: big\ndescription: d\n---\n' }),
		).markdown;
		const padding = 'a'.repeat(SKILL_BODY_MAX_BYTES - byteLength(header) + 1);
		const converted = convertSkill(
			makeSkill({
				dir_name: 'big',
				content: `---\nname: big\ndescription: d\n---\n${padding}`,
			}),
		);
		expect(converted.bytes).toBe(SKILL_BODY_MAX_BYTES + 1);
		expect(converted.blockedReason).toBe('17 KiB, limit 16 KiB');
	});

	it('counts multi-byte bodies in UTF-8 bytes', () => {
		const converted = convertSkill(
			makeSkill({
				content: `---\nname: triage\ndescription: d\n---\n${'я'.repeat(9000)}\n`,
			}),
		);
		expect(converted.blockedReason).toMatch(/KiB, limit 16 KiB/);
	});

	it('reports an invalid slug override through the validator', () => {
		const converted = convertSkill(makeSkill({ content: VALID }), 'Bad Slug');
		expect(converted.blockedReason).toMatch(/^slug /);
	});

	it('produces markdown that validates with no errors', () => {
		const samples = [
			makeSkill({ content: VALID }),
			makeSkill({
				content:
					'---\nname: "Ops: deploy"\ndescription: >\n  Ship it\n---\n# Deploy\n',
			}),
			makeSkill({ dir_name: 'plain', content: '# Plain skill\n\nDo it.\n' }),
		];
		for (const sample of samples) {
			const converted = convertSkill(sample);
			expect(converted.blockedReason).toBeNull();
			expect(
				validateSkillMarkdown(converted.markdown, converted.slug),
			).toBeNull();
			expect(parseSkillFrontMatter(converted.markdown)).toEqual({
				slug: converted.slug,
				title: converted.title,
				when: converted.when,
				actions: [],
			});
		}
	});
});

describe('index', () => {
	it('namespaces the key by API base and user', () => {
		expect(indexStorageKey('https://api.tmgr.dev/api/', 7)).toBe(
			'tmgr.skillImport.v1:https://api.tmgr.dev/api/:7',
		);
		expect(indexStorageKey('https://a/api/', 7)).not.toBe(
			indexStorageKey('https://b/api/', 7),
		);
		expect(indexStorageKey('https://a/api/', 7)).not.toBe(
			indexStorageKey('https://a/api/', 8),
		);
	});

	it('loads an empty index when nothing is stored', () => {
		expect(loadIndex(memoryStorage().storage, 'k')).toEqual({});
	});

	it('loads an empty index from corrupt JSON', () => {
		const { storage } = memoryStorage({ k: '{not json' });
		expect(loadIndex(storage, 'k')).toEqual({});
	});

	it('loads an empty index from a non-object JSON value', () => {
		expect(loadIndex(memoryStorage({ k: '[1,2]' }).storage, 'k')).toEqual({});
		expect(loadIndex(memoryStorage({ k: 'null' }).storage, 'k')).toEqual({});
	});

	it('drops malformed entries and keeps valid ones', () => {
		const { storage } = memoryStorage({
			k: JSON.stringify({ good: ENTRY, bad: { slug: 1 } }),
		});
		expect(loadIndex(storage, 'k')).toEqual({ good: ENTRY });
	});

	it('round-trips through save and load', () => {
		const { storage, data } = memoryStorage();
		const index = recordImport({}, 'claude:personal:triage', ENTRY);
		expect(saveIndex(storage, 'k', index)).toBe(true);
		expect(JSON.parse(data.k)).toEqual(index);
		expect(loadIndex(storage, 'k')).toEqual(index);
	});

	it('reports a failed save instead of throwing', () => {
		const failing: KeyValueStorage = {
			getItem: () => null,
			setItem: () => {
				throw new Error('quota');
			},
		};
		expect(saveIndex(failing, 'k', {})).toBe(false);
	});

	it('records an import without mutating the previous index', () => {
		const before = { a: ENTRY };
		const after = recordImport(before, 'b', { ...ENTRY, slug: 'other' });
		expect(before).toEqual({ a: ENTRY });
		expect(Object.keys(after)).toEqual(['a', 'b']);
	});
});

describe('rowStatus', () => {
	const VALID = '---\nname: triage\ndescription: d\n---\nbody\n';
	const skill = makeSkill({ content: VALID, hash: 'hash-1' });
	const converted = convertSkill(skill);
	const blocked = convertSkill(makeSkill({ content: '', secret: true }));

	it('is blocked whenever the conversion is blocked', () => {
		expect(
			rowStatus({
				skill: makeSkill({ secret: true }),
				converted: blocked,
				entry: ENTRY,
				library: [{ slug: 'triage', version: 3 }],
			}),
		).toBe('blocked');
	});

	it('is new when the library has no such slug and there is no entry', () => {
		expect(
			rowStatus({ skill, converted, library: [{ slug: 'other', version: 1 }] }),
		).toBe('new');
	});

	it('is slug-taken when the library has the slug with a different body', () => {
		expect(
			rowStatus({
				skill,
				converted,
				library: [{ slug: 'triage', version: 1 }],
				libraryBodyMatches: false,
			}),
		).toBe('slug-taken');
	});

	it('is up-to-date when the library body already matches, without an entry', () => {
		expect(
			rowStatus({
				skill,
				converted,
				library: [{ slug: 'triage', version: 1 }],
				libraryBodyMatches: true,
			}),
		).toBe('up-to-date');
	});

	it('is previously-imported-deleted when the entry slug is gone from the library', () => {
		expect(rowStatus({ skill, converted, entry: ENTRY, library: [] })).toBe(
			'previously-imported-deleted',
		);
	});

	it('is edited-in-library when the library version differs from the entry', () => {
		expect(
			rowStatus({
				skill,
				converted,
				entry: ENTRY,
				library: [{ slug: 'triage', version: 4 }],
			}),
		).toBe('edited-in-library');
	});

	it('is up-to-date when the source hash matches the entry', () => {
		expect(
			rowStatus({
				skill,
				converted,
				entry: ENTRY,
				library: [{ slug: 'triage', version: 3 }],
			}),
		).toBe('up-to-date');
	});

	it('is changed when the source hash differs from the entry', () => {
		expect(
			rowStatus({
				skill: makeSkill({ content: VALID, hash: 'hash-2' }),
				converted,
				entry: ENTRY,
				library: [{ slug: 'triage', version: 3 }],
			}),
		).toBe('changed');
	});

	it('ticks new, changed and previously-deleted rows only', () => {
		expect(defaultTicked('new')).toBe(true);
		expect(defaultTicked('changed')).toBe(true);
		expect(defaultTicked('previously-imported-deleted')).toBe(true);
		expect(defaultTicked('up-to-date')).toBe(false);
		expect(defaultTicked('edited-in-library')).toBe(false);
		expect(defaultTicked('slug-taken')).toBe(false);
		expect(defaultTicked('blocked')).toBe(false);
	});
});

describe('planImport', () => {
	const item = (
		skillKey: string,
		slug: string,
		status: Parameters<typeof planImport>[0][number]['status'] = 'new',
		ticked = true,
	) => ({ skillKey, slug, status, ticked });

	it('imports ticked new rows and skips unticked, blocked and up-to-date ones', () => {
		const plan = planImport(
			[
				item('a', 'a'),
				item('b', 'b', 'new', false),
				item('c', 'c', 'blocked'),
				item('d', 'd', 'up-to-date'),
			],
			[],
		);
		expect(plan).toEqual({ toImport: ['a'], blocked: [] });
	});

	it('blocks the second ticked row that uses an already selected slug', () => {
		const plan = planImport(
			[item('a', 'same'), item('b', 'same', 'changed')],
			[],
		);
		expect(plan.toImport).toEqual(['a']);
		expect(plan.blocked).toEqual([
			{ skillKey: 'b', reason: 'duplicate slug in selection' },
		]);
	});

	it('does not count an unticked duplicate as a clash', () => {
		const plan = planImport(
			[item('a', 'same', 'new', false), item('b', 'same')],
			[],
		);
		expect(plan).toEqual({ toImport: ['b'], blocked: [] });
	});

	it('blocks new slugs once the library limit would be exceeded', () => {
		const plan = planImport(
			[item('a', 'a'), item('b', 'b'), item('c', 'c')],
			['x', 'y'],
			4,
		);
		expect(plan.toImport).toEqual(['a', 'b']);
		expect(plan.blocked).toEqual([
			{ skillKey: 'c', reason: 'library limit reached' },
		]);
	});

	it('does not count slugs already in the library against the limit', () => {
		const plan = planImport(
			[item('a', 'x', 'slug-taken'), item('b', 'b')],
			['x', 'y'],
			3,
		);
		expect(plan).toEqual({ toImport: ['a', 'b'], blocked: [] });
	});

	it('uses the shared skill limit of 100 by default', () => {
		const library = Array.from({ length: 100 }, (_, i) => `s${i}`);
		const plan = planImport([item('a', 'new-one')], library);
		expect(plan.blocked).toEqual([
			{ skillKey: 'a', reason: 'library limit reached' },
		]);
	});
});

describe('importSummary', () => {
	it('reports a plural success count', () => {
		expect(importSummary([{ ok: true }, { ok: true }])).toBe(
			'Imported 2 skills',
		);
	});

	it('uses the singular for one skill', () => {
		expect(importSummary([{ ok: true }])).toBe('Imported 1 skill');
	});

	it('appends the failure count', () => {
		expect(importSummary([{ ok: true }, { ok: false }])).toBe(
			'Imported 1 skill, 1 failed',
		);
		expect(importSummary([{ ok: false }, { ok: false }])).toBe(
			'Imported 0 skills, 2 failed',
		);
	});
});
