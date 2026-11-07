import {
	ATTACHED_SKILL_LIMIT,
	parseSkillFrontMatter,
	SHARED_SKILL_LIMIT,
	SKILL_SLUG_PATTERN,
	validateSkillMarkdown,
} from '@/utils/personas';
import { type IndexEntry, isIndexEntry } from '@/utils/skillImport';
import {
	checkSkillFiles,
	EMPTY_FILES_SHA256,
	type SkillFileEntry,
	type SkillFileSet,
} from './skillFileSet';

export interface LibrarySkill {
	slug: string;
	title: string;
	when: string;
	markdown: string;
	version: number;
	updated_at: string;
	files?: SkillFileEntry[];
	files_sha256?: string;
	[extra: string]: unknown;
}

export const skillFilesSha256 = (skill: LibrarySkill): string =>
	skill.files?.length
		? (skill.files_sha256 ?? EMPTY_FILES_SHA256)
		: EMPTY_FILES_SHA256;

export interface LibraryAttachment {
	persona_uuid: string;
	slug: string;
}

export interface LibraryFile {
	version: 1;
	skills: LibrarySkill[];
	links: Record<string, IndexEntry>;
	attachments: LibraryAttachment[];
	[extra: string]: unknown;
}

export interface SkillRef {
	slug: string;
	title: string;
	when: string;
	version: number;
}

export interface MergedSkill extends SkillRef {
	source: 'own' | 'shared';
}

export interface SkillBody {
	slug: string;
	body: string;
	version: number;
	source: 'shared';
	files: { path: string; size: number }[];
	files_sha256: string;
}

export interface SkillImportSkipped {
	slug: string;
	reason: string;
}

export interface SkillImportResult {
	library: LibraryFile;
	imported: string[];
	skipped: SkillImportSkipped[];
}

export class SkillLibraryError extends Error {}

type SkillCheck =
	{ ok: true; title: string; when: string } | { ok: false; reason: string };

const isRecord = (value: unknown): value is Record<string, unknown> =>
	typeof value === 'object' && value !== null && !Array.isArray(value);

const errorText = (errors: Record<string, string[]>): string =>
	Object.entries(errors)
		.map(([field, messages]) => `${field} ${messages.join(', ')}`)
		.join('; ');

const checkSkill = (slug: string, markdown: string): SkillCheck => {
	if (!SKILL_SLUG_PATTERN.test(slug)) {
		return { ok: false, reason: 'slug must be lowercase letters, digits or -' };
	}
	const errors = validateSkillMarkdown(markdown, slug);
	const parsed = parseSkillFrontMatter(markdown);
	if (errors || !parsed) {
		return {
			ok: false,
			reason: errors ? errorText(errors) : 'invalid front matter',
		};
	}
	return { ok: true, title: parsed.title, when: parsed.when };
};

const attachedSkills = (
	lib: LibraryFile,
	personaUuid: string,
): LibrarySkill[] =>
	lib.attachments
		.filter((attachment) => attachment.persona_uuid === personaUuid)
		.flatMap((attachment) => {
			const skill = lib.skills.find((item) => item.slug === attachment.slug);
			return skill ? [skill] : [];
		});

export const emptyLibrary = (): LibraryFile => ({
	version: 1,
	skills: [],
	links: {},
	attachments: [],
});

export interface LibraryReport {
	library: LibraryFile;
	skipped: SkillImportSkipped[];
}

export const validateLibraryReport = (input: unknown): LibraryReport => {
	if (!isRecord(input) || input.version !== 1) {
		throw new SkillLibraryError('library must have version 1');
	}
	if (!Array.isArray(input.skills)) {
		throw new SkillLibraryError('skills must be an array');
	}
	if (input.skills.length > SHARED_SKILL_LIMIT) {
		throw new SkillLibraryError(
			`library holds at most ${SHARED_SKILL_LIMIT} skills`,
		);
	}

	const skipped: SkillImportSkipped[] = [];
	const skills: LibrarySkill[] = [];
	const seen = new Set<string>();
	for (const value of input.skills) {
		if (!isRecord(value)) {
			skipped.push({ slug: '', reason: 'skill must be an object' });
			continue;
		}
		const { slug, markdown, version, updated_at: updatedAt } = value;
		if (
			typeof slug !== 'string' ||
			typeof markdown !== 'string' ||
			typeof updatedAt !== 'string' ||
			!Number.isInteger(version) ||
			(version as number) < 1
		) {
			skipped.push({
				slug: typeof slug === 'string' ? slug : '',
				reason: 'skill is malformed',
			});
			continue;
		}
		const checked = checkSkill(slug, markdown);
		if (!checked.ok) {
			skipped.push({ slug, reason: checked.reason });
			continue;
		}
		if (seen.has(slug)) {
			skipped.push({ slug, reason: 'duplicate slug' });
			continue;
		}
		let fileSet: SkillFileSet | null = null;
		if (value.files !== undefined || value.files_sha256 !== undefined) {
			const files = checkSkillFiles(value.files, value.files_sha256);
			if (!files.ok) {
				skipped.push({ slug, reason: files.reason });
				continue;
			}
			fileSet = files.set;
		}
		seen.add(slug);
		skills.push({
			...value,
			...(fileSet && { files: fileSet.files, files_sha256: fileSet.sha256 }),
			slug,
			title: checked.title,
			when: checked.when,
			markdown,
			version: version as number,
			updated_at: updatedAt,
		});
	}
	const dropped = new Set(
		skipped.map((item) => item.slug).filter((slug) => !seen.has(slug)),
	);

	const linksInput = input.links ?? {};
	if (!isRecord(linksInput)) {
		throw new SkillLibraryError('links must be an object');
	}
	const links: Record<string, IndexEntry> = {};
	for (const [key, entry] of Object.entries(linksInput)) {
		if (!isIndexEntry(entry)) {
			throw new SkillLibraryError(`link ${key} is malformed`);
		}
		if (!dropped.has(entry.slug)) links[key] = entry;
	}

	const attachmentsInput = input.attachments ?? [];
	if (!Array.isArray(attachmentsInput)) {
		throw new SkillLibraryError('attachments must be an array');
	}
	const attachments: LibraryAttachment[] = [];
	for (const value of attachmentsInput) {
		if (
			!isRecord(value) ||
			typeof value.persona_uuid !== 'string' ||
			typeof value.slug !== 'string'
		) {
			throw new SkillLibraryError('attachment is malformed');
		}
		if (dropped.has(value.slug)) continue;
		attachments.push({
			...value,
			persona_uuid: value.persona_uuid,
			slug: value.slug,
		});
	}
	const perPersona = new Map<string, number>();
	for (const attachment of attachments) {
		const count = (perPersona.get(attachment.persona_uuid) ?? 0) + 1;
		if (count > ATTACHED_SKILL_LIMIT) {
			throw new SkillLibraryError(
				`a persona attaches at most ${ATTACHED_SKILL_LIMIT} skills`,
			);
		}
		perPersona.set(attachment.persona_uuid, count);
	}

	return {
		library: { ...input, version: 1, skills, links, attachments },
		skipped,
	};
};

export const validateLibrary = (input: unknown): LibraryFile =>
	validateLibraryReport(input).library;

export const putSkill = (
	lib: LibraryFile,
	slug: string,
	markdown: string,
	now: string,
	fileSet?: SkillFileSet,
): { library: LibraryFile; skill: LibrarySkill } => {
	const checked = checkSkill(slug, markdown);
	if (!checked.ok) throw new SkillLibraryError(checked.reason);
	if (fileSet) {
		const files = checkSkillFiles(fileSet.files, fileSet.sha256);
		if (!files.ok) throw new SkillLibraryError(files.reason);
	}

	const existing = lib.skills.find((skill) => skill.slug === slug);
	const sameFiles =
		!fileSet ||
		fileSet.sha256 ===
			(existing ? skillFilesSha256(existing) : EMPTY_FILES_SHA256);
	if (existing && existing.markdown === markdown && sameFiles) {
		return { library: lib, skill: existing };
	}
	if (!existing && lib.skills.length >= SHARED_SKILL_LIMIT) {
		throw new SkillLibraryError(
			`library holds at most ${SHARED_SKILL_LIMIT} skills`,
		);
	}

	const bodyChanged = !existing || existing.markdown !== markdown;
	const {
		files: keptFiles,
		files_sha256: keptSha,
		...base
	} = existing ?? ({} as Partial<LibrarySkill>);
	const nextFiles =
		fileSet ??
		(keptFiles && keptSha ? { files: keptFiles, sha256: keptSha } : undefined);
	const skill: LibrarySkill = {
		...base,
		...(nextFiles?.files.length
			? { files: nextFiles.files, files_sha256: nextFiles.sha256 }
			: {}),
		slug,
		title: checked.title,
		when: checked.when,
		markdown,
		version: existing ? existing.version + (bodyChanged ? 1 : 0) : 1,
		updated_at: now,
	};
	const skills = existing
		? lib.skills.map((item) => (item.slug === slug ? skill : item))
		: [...lib.skills, skill];
	return { library: { ...lib, skills }, skill };
};

export const deleteSkill = (lib: LibraryFile, slug: string): LibraryFile => ({
	...lib,
	skills: lib.skills.filter((skill) => skill.slug !== slug),
	links: Object.fromEntries(
		Object.entries(lib.links).filter(([, entry]) => entry.slug !== slug),
	),
	attachments: lib.attachments.filter((attachment) => attachment.slug !== slug),
});

export const attach = (
	lib: LibraryFile,
	personaUuid: string,
	slug: string,
): LibraryFile => {
	if (!lib.skills.some((skill) => skill.slug === slug)) {
		throw new SkillLibraryError(`skill ${slug} does not exist`);
	}
	if (
		lib.attachments.some(
			(attachment) =>
				attachment.persona_uuid === personaUuid && attachment.slug === slug,
		)
	) {
		return lib;
	}
	const attachedCount = lib.attachments.filter(
		(attachment) => attachment.persona_uuid === personaUuid,
	).length;
	if (attachedCount >= ATTACHED_SKILL_LIMIT) {
		throw new SkillLibraryError(
			`a persona attaches at most ${ATTACHED_SKILL_LIMIT} skills`,
		);
	}
	return {
		...lib,
		attachments: [...lib.attachments, { persona_uuid: personaUuid, slug }],
	};
};

export const detach = (
	lib: LibraryFile,
	personaUuid: string,
	slug: string,
): LibraryFile => {
	const attachments = lib.attachments.filter(
		(attachment) =>
			!(attachment.persona_uuid === personaUuid && attachment.slug === slug),
	);
	return attachments.length === lib.attachments.length
		? lib
		: { ...lib, attachments };
};

export const mergeSkills = (
	own: SkillRef[],
	shared: SkillRef[],
): MergedSkill[] => {
	const ownSlugs = new Set(own.map((skill) => skill.slug));
	const merged: MergedSkill[] = [
		...own.map(({ slug, title, when, version }): MergedSkill => ({
			slug,
			title,
			when,
			version,
			source: 'own',
		})),
		...shared
			.filter((skill) => !ownSlugs.has(skill.slug))
			.map(({ slug, title, when, version }): MergedSkill => ({
				slug,
				title,
				when,
				version,
				source: 'shared',
			})),
	];
	return merged.sort((a, b) =>
		a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0,
	);
};

export const personaSkillIndex = (
	lib: LibraryFile,
	personaUuid: string,
): (MergedSkill & { files_sha256: string })[] =>
	attachedSkills(lib, personaUuid)
		.map((skill) => ({
			slug: skill.slug,
			title: skill.title,
			when: skill.when,
			version: skill.version,
			source: 'shared' as const,
			files_sha256: skillFilesSha256(skill),
		}))
		.sort((a, b) => (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0));

export const personaSkillBody = (
	lib: LibraryFile,
	personaUuid: string,
	slug: string,
): SkillBody | null => {
	const skill = attachedSkills(lib, personaUuid).find(
		(item) => item.slug === slug,
	);
	return skill
		? {
				slug,
				body: skill.markdown,
				version: skill.version,
				source: 'shared',
				files: (skill.files ?? []).map(({ path, size }) => ({ path, size })),
				files_sha256: skillFilesSha256(skill),
			}
		: null;
};

export const personaSkillFile = (
	lib: LibraryFile,
	personaUuid: string,
	slug: string,
	path: string,
): {
	slug: string;
	path: string;
	content: string;
	size: number;
	sha256: string;
} | null => {
	const skill = attachedSkills(lib, personaUuid).find(
		(item) => item.slug === slug,
	);
	const file = skill?.files?.find((item) => item.path === path);
	return file
		? {
				slug,
				path: file.path,
				content: file.content,
				size: file.size,
				sha256: file.sha256,
			}
		: null;
};

export const exportProjection = (
	lib: LibraryFile,
): { version: 1; skills: LibrarySkill[] } => ({
	version: 1,
	skills: lib.skills,
});

export const importSkills = (
	lib: LibraryFile,
	incoming: unknown,
	now: string,
): SkillImportResult => {
	if (
		!isRecord(incoming) ||
		incoming.version !== 1 ||
		!Array.isArray(incoming.skills)
	) {
		throw new SkillLibraryError('import must be {version: 1, skills: []}');
	}

	let library = lib;
	const imported: string[] = [];
	const skipped: SkillImportSkipped[] = [];
	for (const entry of incoming.skills) {
		if (
			!isRecord(entry) ||
			typeof entry.slug !== 'string' ||
			typeof entry.markdown !== 'string'
		) {
			skipped.push({ slug: '', reason: 'invalid entry' });
			continue;
		}
		const { slug, markdown } = entry;
		if (library.skills.some((skill) => skill.slug === slug)) {
			skipped.push({ slug, reason: 'exists' });
			continue;
		}
		try {
			const fileSet =
				entry.files === undefined
					? undefined
					: ({
							files: entry.files,
							sha256: entry.files_sha256,
						} as SkillFileSet);
			library = putSkill(library, slug, markdown, now, fileSet).library;
			imported.push(slug);
		} catch (error) {
			if (!(error instanceof SkillLibraryError)) throw error;
			skipped.push({ slug, reason: error.message });
		}
	}
	return { library, imported, skipped };
};
