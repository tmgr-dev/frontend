import type { SkillOutcome } from '../api';
import {
	batchSkillsPayload,
	buildSkillsPayload,
	dryRunSkillsLine,
	skillsReportLines,
} from '../skills';

const skill = (slug: string) => ({
	slug,
	title: slug,
	when: '',
	markdown: `---\nslug: ${slug}\n---\nbody`,
	version: 1,
	updated_at: '2026-11-06T00:00:00Z',
});

const outcome = (extra: Partial<SkillOutcome>): SkillOutcome => ({
	slug: 'a',
	cloud_slug: 'a',
	status: 'created',
	reason: null,
	attached: [],
	skipped_attachments: [],
	...extra,
});

describe('buildSkillsPayload', () => {
	const library = {
		skills: [skill('a'), skill('b'), skill('c')],
		attachments: [
			{ persona_uuid: 'p1', slug: 'a' },
			{ persona_uuid: 'p1', slug: 'b' },
			{ persona_uuid: 'p2', slug: 'b' },
			{ persona_uuid: 'other', slug: 'c' },
			{ persona_uuid: 'p1', slug: 'ghost' },
		],
	};

	it('keeps skills attached to workspace personas, with their personas', () => {
		expect(buildSkillsPayload(library, ['p1', 'p2'])).toEqual({
			skills: [
				{ markdown: library.skills[0].markdown, attach_to: ['p1'], files: [] },
				{
					markdown: library.skills[1].markdown,
					attach_to: ['p1', 'p2'],
					files: [],
				},
			],
		});
	});

	it('drops personas outside the workspace', () => {
		expect(
			buildSkillsPayload(library, ['p2']).skills.map((s) => s.attach_to),
		).toEqual([['p2']]);
	});

	it('is empty without a library or attachments', () => {
		expect(buildSkillsPayload(null, ['p1'])).toEqual({ skills: [] });
		expect(
			buildSkillsPayload({ skills: [skill('a')], attachments: [] }, ['p1']),
		).toEqual({ skills: [] });
	});
});

describe('skillsReportLines', () => {
	it('summarises added, reused and renamed skills in one line', () => {
		const lines = skillsReportLines([
			outcome({ slug: 'a' }),
			outcome({ slug: 'b' }),
			outcome({ slug: 'c', status: 'reused' }),
			outcome({ slug: 'review', cloud_slug: 'review-2', status: 'renamed' }),
		]);
		expect(lines).toEqual([
			'Skills: 2 added, 1 already in your library, 1 renamed (review → review-2)',
		]);
	});

	it('lists skipped skills and attachments with a reason', () => {
		expect(
			skillsReportLines([
				outcome({
					slug: 'x',
					cloud_slug: null,
					status: 'skipped',
					reason: 'library_full',
				}),
				outcome({
					slug: 'y',
					skipped_attachments: [
						{ persona_uuid: 'p1', reason: 'not_moved' },
						{ persona_uuid: 'p2', reason: 'not_moved' },
					],
				}),
			]),
		).toEqual([
			'Skills: 1 added',
			'x: not copied, your library is full',
			'y: not attached to 2 personas, the persona was not moved',
		]);
	});

	it('falls back when the slug is missing', () => {
		expect(
			skillsReportLines([
				outcome({
					slug: null,
					cloud_slug: 'z',
					status: 'skipped',
					reason: 'invalid',
				}),
				outcome({
					slug: null,
					cloud_slug: null,
					status: 'skipped',
					reason: 'invalid',
				}),
			]),
		).toEqual([
			'z: not copied, not a valid skill',
			'a skill: not copied, not a valid skill',
		]);
	});

	it('is empty for an empty report', () => {
		expect(skillsReportLines([])).toEqual([]);
		expect(skillsReportLines(null)).toEqual([]);
	});

	it('words the dry run line', () => {
		expect(dryRunSkillsLine(3)).toBe("Skills: 3 from this device's library");
	});
});

describe('skill files in the payload', () => {
	const withFiles = {
		skills: [
			{
				...skill('a'),
				files: [
					{ path: 'run.sh', content: 'echo', size: 4, sha256: 'x' },
					{ path: 'references/g.md', content: 'g', size: 1, sha256: 'y' },
				],
			},
		],
		attachments: [{ persona_uuid: 'p1', slug: 'a' }],
	};

	it('sends path and content only', () => {
		expect(buildSkillsPayload(withFiles, ['p1']).skills[0].files).toEqual([
			{ path: 'run.sh', content: 'echo' },
			{ path: 'references/g.md', content: 'g' },
		]);
	});

	it('splits a big payload into batches under the request limit', () => {
		const big = (n: number) => ({
			markdown: 'm',
			attach_to: ['p1'],
			files: [{ path: `f${n}`, content: 'a'.repeat(900_000) }],
		});
		const { batches } = batchSkillsPayload([big(1), big(2), big(3)]);
		expect(batches.map((b) => b.length)).toEqual([1, 1, 1]);
		expect(batchSkillsPayload([])).toEqual({ batches: [], tooLarge: [] });
		const many = Array.from({ length: 101 }, () => ({
			markdown: 'm',
			attach_to: [],
			files: [],
		}));
		expect(batchSkillsPayload(many).batches.map((b) => b.length)).toEqual([
			100, 1,
		]);
	});

	it('keeps back a skill whose serialized size is over the limit, measured after JSON escaping', () => {
		const content = '\u0001'.repeat(60 * 1024);
		expect(content.length).toBeLessThan(64 * 1024);
		const heavy = {
			markdown: 'm',
			attach_to: ['p1'],
			files: Array.from({ length: 5 }, (_, i) => ({ path: `f${i}`, content })),
		};
		const light = { markdown: 'm', attach_to: ['p1'], files: [] };
		const out = batchSkillsPayload([light, heavy]);
		expect(out.tooLarge).toEqual([1]);
		expect(out.batches).toEqual([[light]]);
		expect(
			skillsReportLines([
				outcome({ slug: 'a', status: 'skipped', reason: 'too_large' }),
			]),
		).toEqual(['a: not copied, too large to move to cloud']);
	});

	it('counts the request wrapper and separators so a batch never exceeds the limit', () => {
		const wrapper = JSON.stringify({ skills: [] }).length;
		const make = (bytes: number) => {
			const base = {
				markdown: 'm',
				attach_to: [],
				files: [{ path: 'f', content: '' }],
			};
			const empty = JSON.stringify(base).length;
			return {
				...base,
				files: [{ path: 'f', content: 'a'.repeat(bytes - empty) }],
			};
		};
		const justUnder = make(1_500_000 - wrapper);
		expect(JSON.stringify(justUnder).length + wrapper).toBe(1_500_000);
		expect(batchSkillsPayload([justUnder]).batches).toEqual([[justUnder]]);
		const small = make(200);
		const out = batchSkillsPayload([make(1_500_000 - wrapper - 100), small]);
		expect(out.batches.map((b) => b.length)).toEqual([1, 1]);
		const over = make(1_500_000 - wrapper + 1);
		expect(batchSkillsPayload([over])).toEqual({ batches: [], tooLarge: [0] });
		for (const batch of batchSkillsPayload([justUnder, small, small, small])
			.batches) {
			expect(JSON.stringify({ skills: batch }).length).toBeLessThanOrEqual(
				1_500_000,
			);
		}
	});

	it('explains invalid_files and files_limit per skill', () => {
		const lines = skillsReportLines([
			outcome({ slug: 'a', status: 'skipped', reason: 'invalid_files' }),
			outcome({ slug: 'b', status: 'skipped', reason: 'files_limit' }),
		]);
		expect(lines).toEqual([
			'a: not copied, its files are not valid (path, text or size limits)',
			'b: not copied, its files would exceed your 10 MiB files limit',
		]);
	});
});
