import type { SkillOutcome } from '../api';
import {
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
				{ markdown: library.skills[0].markdown, attach_to: ['p1'] },
				{ markdown: library.skills[1].markdown, attach_to: ['p1', 'p2'] },
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
