import { convertSkill, rowStatus, type ScanResult } from '@/utils/skillImport';
import type { LibraryFile } from './skillLibrary';

export type SkillUpdateStatus =
	'update-available' | 'edited-and-changed' | 'not-scanned';

export const linkedUpdateStatuses = (
	lib: LibraryFile,
	scan: ScanResult,
): Record<string, SkillUpdateStatus> => {
	const scanned = new Map(
		scan.sources.flatMap((source) =>
			source.packages.flatMap((pack) =>
				pack.skills.map((skill) => [skill.key, skill] as const),
			),
		),
	);
	const refs = lib.skills.map(({ slug, version }) => ({ slug, version }));
	const result: Record<string, SkillUpdateStatus> = {};
	for (const [key, entry] of Object.entries(lib.links)) {
		const skill = scanned.get(key);
		if (!skill) {
			result[entry.slug] ??= 'not-scanned';
			continue;
		}
		const status = rowStatus({
			skill,
			converted: convertSkill(skill, entry.slug),
			entry,
			library: refs,
		});
		if (status === 'changed') {
			result[entry.slug] = 'update-available';
		} else if (
			status === 'edited-in-library' &&
			skill.hash !== entry.sourceHash
		) {
			result[entry.slug] = 'edited-and-changed';
		} else if (result[entry.slug] === 'not-scanned') {
			delete result[entry.slug];
		}
	}
	return result;
};
