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
			})),
	};
};

const SKILL_REASONS: Record<string, string> = {
	library_full: 'your library is full',
	invalid: 'not a valid skill',
	not_owner: 'the persona belongs to another member',
	not_moved: 'the persona was not moved',
	attach_limit: 'the persona has too many skills',
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
