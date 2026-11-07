import type { LibraryFile } from '../skillLibrary';
import type { SkillOutcome, SkillsPayload, SkillsReport } from './api';

export const buildSkillsPayload = (
	library: Pick<LibraryFile, 'skills' | 'attachments'> | null | undefined,
	personaUuids: Iterable<string>,
): SkillsPayload => {
	const moved = new Set(personaUuids);
	const targets = new Map<string, string[]>();
	for (const { slug, persona_uuid } of library?.attachments ?? []) {
		if (!moved.has(persona_uuid)) continue;
		const list = targets.get(slug) ?? [];
		if (!list.includes(persona_uuid)) list.push(persona_uuid);
		targets.set(slug, list);
	}
	return {
		skills: (library?.skills ?? [])
			.filter((skill) => targets.has(skill.slug))
			.map((skill) => ({
				markdown: skill.markdown,
				attach_to: targets.get(skill.slug)!,
				files: (skill.files ?? []).map(({ path, content }) => ({
					path,
					content,
				})),
			})),
	};
};

const MAX_BATCH_BYTES = 1_500_000;
const MAX_BATCH_SKILLS = 100;

export const batchSkillsPayload = (
	skills: SkillsPayload['skills'],
): {
	batches: SkillsPayload['skills'][];
	tooLarge: SkillsPayload['skills'];
} => {
	const batches: SkillsPayload['skills'][] = [];
	const tooLarge: SkillsPayload['skills'] = [];
	let current: SkillsPayload['skills'] = [];
	let bytes = 0;
	for (const skill of skills) {
		const size = new TextEncoder().encode(JSON.stringify(skill)).length;
		if (size > MAX_BATCH_BYTES) {
			tooLarge.push(skill);
			continue;
		}
		if (
			current.length > 0 &&
			(bytes + size > MAX_BATCH_BYTES || current.length >= MAX_BATCH_SKILLS)
		) {
			batches.push(current);
			current = [];
			bytes = 0;
		}
		current.push(skill);
		bytes += size;
	}
	if (current.length > 0) batches.push(current);
	return { batches, tooLarge };
};

const SKILL_REASONS: Record<string, string> = {
	library_full: 'your library is full',
	invalid: 'not a valid skill',
	not_owner: 'the persona belongs to another member',
	not_moved: 'the persona was not moved',
	attach_limit: 'the persona has too many skills',
	invalid_files: 'its files are not valid (path, text or size limits)',
	too_large: 'too large to move to cloud',
	files_limit: 'its files would exceed your 10 MiB files limit',
};

const skillReason = (reason: string | null) =>
	reason ? (SKILL_REASONS[reason] ?? reason.replace(/_/g, ' ')) : '';

const name = (o: SkillOutcome) => o.slug ?? o.cloud_slug ?? 'a skill';

export const skillsReportLines = (
	report: SkillsReport | null | undefined,
): string[] => {
	if (!report?.length) return [];
	const by = (status: SkillOutcome['status']) =>
		report.filter((o) => o.status === status);
	const parts: string[] = [];
	if (by('created').length) parts.push(`${by('created').length} added`);
	if (by('reused').length)
		parts.push(`${by('reused').length} already in your library`);
	const renamed = by('renamed');
	if (renamed.length)
		parts.push(
			`${renamed.length} renamed (${renamed
				.map((o) => `${name(o)} → ${o.cloud_slug}`)
				.join(', ')})`,
		);
	const lines = parts.length ? [`Skills: ${parts.join(', ')}`] : [];
	for (const outcome of report) {
		if (outcome.status === 'skipped') {
			lines.push(
				`${name(outcome)}: not copied, ${skillReason(outcome.reason)}`,
			);
		}
		const reasons = new Map<string, number>();
		for (const { reason } of outcome.skipped_attachments ?? [])
			reasons.set(reason, (reasons.get(reason) ?? 0) + 1);
		for (const [reason, n] of reasons) {
			lines.push(
				`${name(outcome)}: not attached to ${n} ${
					n === 1 ? 'persona' : 'personas'
				}, ${skillReason(reason)}`,
			);
		}
	}
	return lines;
};

export const dryRunSkillsLine = (count: number): string =>
	`Skills: ${count} from this device's library`;
