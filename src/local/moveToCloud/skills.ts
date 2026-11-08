import { t } from '@/i18n';
import type { LibraryFile } from '../skillLibrary';
import type { SkillOutcome, SkillsPayload, SkillsReport } from './api';

export const buildSkillsPayloadWithSlugs = (
	library: Pick<LibraryFile, 'skills' | 'attachments'> | null | undefined,
	personaUuids: Iterable<string>,
): { payload: SkillsPayload; slugs: string[] } => {
	const moved = new Set(personaUuids);
	const targets = new Map<string, string[]>();
	for (const { slug, persona_uuid } of library?.attachments ?? []) {
		if (!moved.has(persona_uuid)) continue;
		const list = targets.get(slug) ?? [];
		if (!list.includes(persona_uuid)) list.push(persona_uuid);
		targets.set(slug, list);
	}
	const picked = (library?.skills ?? []).filter((skill) =>
		targets.has(skill.slug),
	);
	return {
		payload: {
			skills: picked.map((skill) => ({
				markdown: skill.markdown,
				attach_to: targets.get(skill.slug)!,
				files: (skill.files ?? []).map(({ path, content }) => ({
					path,
					content,
				})),
			})),
		},
		slugs: picked.map((skill) => skill.slug),
	};
};

export const buildSkillsPayload = (
	library: Pick<LibraryFile, 'skills' | 'attachments'> | null | undefined,
	personaUuids: Iterable<string>,
): SkillsPayload => buildSkillsPayloadWithSlugs(library, personaUuids).payload;

const MAX_BATCH_BYTES = 1_500_000;
const MAX_BATCH_SKILLS = 100;

const encodedLength = (value: unknown): number =>
	new TextEncoder().encode(JSON.stringify(value)).length;

const WRAPPER_BYTES = encodedLength({ skills: [] });

export const batchSkillsPayload = (
	skills: SkillsPayload['skills'],
): {
	batches: SkillsPayload['skills'][];
	tooLarge: number[];
} => {
	const batches: SkillsPayload['skills'][] = [];
	const tooLarge: number[] = [];
	let current: SkillsPayload['skills'] = [];
	let bytes = WRAPPER_BYTES;
	skills.forEach((skill, index) => {
		const size = encodedLength(skill);
		if (WRAPPER_BYTES + size > MAX_BATCH_BYTES) {
			tooLarge.push(index);
			return;
		}
		const separator = current.length > 0 ? 1 : 0;
		if (
			current.length > 0 &&
			(bytes + separator + size > MAX_BATCH_BYTES ||
				current.length >= MAX_BATCH_SKILLS)
		) {
			batches.push(current);
			current = [];
			bytes = WRAPPER_BYTES;
		}
		bytes += (current.length > 0 ? 1 : 0) + size;
		current.push(skill);
	});
	if (current.length > 0) batches.push(current);
	return { batches, tooLarge };
};

const SKILL_REASONS = new Set([
	'library_full',
	'invalid',
	'not_owner',
	'not_moved',
	'attach_limit',
	'invalid_files',
	'too_large',
	'files_limit',
]);

const skillReason = (reason: string | null) =>
	reason
		? SKILL_REASONS.has(reason)
			? t(`desktop.helpers.move.skillReasons.${reason}`)
			: reason.replace(/_/g, ' ')
		: '';

const name = (o: SkillOutcome) =>
	o.slug ?? o.cloud_slug ?? t('desktop.helpers.move.skills.aSkill');

export const skillsReportLines = (
	report: SkillsReport | null | undefined,
): string[] => {
	if (!report?.length) return [];
	const by = (status: SkillOutcome['status']) =>
		report.filter((o) => o.status === status);
	const parts: string[] = [];
	if (by('created').length)
		parts.push(
			t('desktop.helpers.move.skills.added', { n: by('created').length }),
		);
	if (by('reused').length)
		parts.push(
			t('desktop.helpers.move.skills.reused', { n: by('reused').length }),
		);
	const renamed = by('renamed');
	if (renamed.length)
		parts.push(
			t('desktop.helpers.move.skills.renamed', {
				n: renamed.length,
				list: renamed.map((o) => `${name(o)} → ${o.cloud_slug}`).join(', '),
			}),
		);
	const lines = parts.length
		? [t('desktop.helpers.move.skills.summary', { parts: parts.join(', ') })]
		: [];
	for (const outcome of report) {
		if (outcome.status === 'skipped') {
			lines.push(
				t('desktop.helpers.move.skills.notCopied', {
					name: name(outcome),
					reason: skillReason(outcome.reason),
				}),
			);
		}
		const reasons = new Map<string, number>();
		for (const { reason } of outcome.skipped_attachments ?? [])
			reasons.set(reason, (reasons.get(reason) ?? 0) + 1);
		for (const [reason, n] of reasons) {
			lines.push(
				t(
					'desktop.helpers.move.skills.notAttached',
					{ name: name(outcome), reason: skillReason(reason), n },
					n,
				),
			);
		}
	}
	return lines;
};

export const dryRunSkillsLine = (count: number): string =>
	t('desktop.helpers.move.skills.dryRun', { count });
