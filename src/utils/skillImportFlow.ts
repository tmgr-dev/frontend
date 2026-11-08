import { t } from '@/i18n';
import { type SkillFileEntry, type SkillFileSet } from '@/local/skillFileSet';
import {
	EMPTY_FILES_SHA256,
	convertSkill,
	defaultTicked,
	planImport,
	recordImport,
	rowStatus,
	suggestSlug,
	textHash,
	uploadableFiles,
	type ConvertedSkill,
	type ImportIndex,
	type LibraryEntryRef,
	type RowStatus,
	type ScanResult,
	type ScanSkill,
} from '@/utils/skillImport';

export const changedSincePreview = (): string =>
	t('agents.helpers.skillImport.flow.changedSincePreview');
export const filesChangedInLibrary = (): string =>
	t('agents.helpers.skillImport.flow.filesChanged');

export interface SkillFilesRead {
	key: string;
	ok: boolean;
	error: string | null;
	files: SkillFileEntry[];
	files_sha256: string;
}

export interface ImportRow {
	skill: ScanSkill;
	sourceId: string;
	sourceLabel: string;
	packageName: string;
	slugOverride: string;
	converted: ConvertedSkill;
	status: RowStatus;
	ticked: boolean;
	bodyMatches: boolean | undefined;
}

export interface RowResult {
	skillKey: string;
	ok: boolean;
	error?: string;
}

const bodyHash = (text: string): string => textHash(text.replace(/\s+$/, ''));

const libraryFilesSha = (library: LibraryEntryRef[], slug: string): string =>
	library.find((item) => item.slug === slug)?.filesSha256 ?? EMPTY_FILES_SHA256;

const sameMarkdown = (a: string, b: string): boolean =>
	a.replace(/\s+$/, '') === b.replace(/\s+$/, '');

export const evaluateRow = (
	row: Pick<ImportRow, 'skill' | 'slugOverride' | 'bodyMatches'>,
	library: LibraryEntryRef[],
	index: ImportIndex,
): { converted: ConvertedSkill; status: RowStatus } => {
	const entry = index[row.skill.key];
	const converted = convertSkill(
		row.skill,
		entry ? entry.slug : row.slugOverride || undefined,
	);
	const status = rowStatus({
		skill: row.skill,
		converted,
		entry,
		library,
		libraryBodyMatches: row.bodyMatches,
	});
	return { converted, status };
};

export const buildRows = async (
	scan: ScanResult,
	library: LibraryEntryRef[],
	index: ImportIndex,
	fetchBody: (slug: string) => Promise<string>,
): Promise<ImportRow[]> => {
	const rows: ImportRow[] = [];
	for (const source of scan.sources) {
		for (const pack of source.packages) {
			for (const skill of pack.skills) {
				const base = {
					skill,
					slugOverride: '',
					bodyMatches: undefined as boolean | undefined,
				};
				let { converted, status } = evaluateRow(base, library, index);
				if (status === 'slug-taken') {
					try {
						base.bodyMatches =
							libraryFilesSha(library, converted.slug) === skill.files_sha256 &&
							sameMarkdown(await fetchBody(converted.slug), converted.markdown);
					} catch {
						base.bodyMatches = false;
					}
					({ converted, status } = evaluateRow(base, library, index));
				}
				rows.push({
					...base,
					sourceId: source.id,
					sourceLabel: source.label,
					packageName: pack.name,
					converted,
					status,
					ticked: defaultTicked(status),
				});
			}
		}
	}
	return rows;
};

export const adoptMatches = (
	rows: ImportRow[],
	library: LibraryEntryRef[],
	index: ImportIndex,
	now: string = new Date().toISOString(),
): ImportIndex => {
	let next = index;
	for (const row of rows) {
		const inLibrary = library.find((item) => item.slug === row.converted.slug);
		if (row.bodyMatches && inLibrary && !next[row.skill.key]) {
			next = recordImport(next, row.skill.key, {
				slug: row.converted.slug,
				sourceHash: row.skill.hash,
				libraryVersion: inLibrary.version,
				importedAt: now,
				contentHash: bodyHash(row.converted.markdown),
				filesSha256: row.skill.files_sha256,
			});
		}
	}
	return next;
};

export const clashSuggestion = (row: ImportRow): string =>
	suggestSlug(row.packageName, row.converted.slug);

export type PutSkill = (
	slug: string,
	markdown: string,
	files: SkillFileSet,
	expectedFilesSha256: string,
) => Promise<{ version: number }>;

export interface ImportDeps {
	listLibrary: () => Promise<LibraryEntryRef[]>;
	put: PutSkill;
	getBody?: (slug: string) => Promise<string>;
	readFiles?: (keys: string[]) => Promise<SkillFilesRead[]>;
	index: ImportIndex;
	now?: () => string;
}

export interface ImportTarget {
	listLibrary: () => Promise<LibraryEntryRef[]>;
	put: PutSkill;
	getBody: (slug: string) => Promise<string>;
	loadIndex: () => ImportIndex | Promise<ImportIndex>;
	saveIndex: (index: ImportIndex) => boolean | Promise<boolean>;
}

export const runImport = async (
	rows: ImportRow[],
	deps: ImportDeps,
): Promise<{
	results: RowResult[];
	index: ImportIndex;
	blocked: { skillKey: string; reason: string }[];
}> => {
	const library = await deps.listLibrary();
	const plan = planImport(
		rows.map((row) => ({
			skillKey: row.skill.key,
			slug: row.converted.slug,
			status: row.status,
			ticked: row.ticked,
		})),
		library.map((item) => item.slug),
	);
	const byKey = new Map(rows.map((row) => [row.skill.key, row]));
	let index = deps.index;
	const results: RowResult[] = [];
	for (const key of plan.toImport) {
		const row = byKey.get(key)!;
		try {
			const stored = index[key];
			if (
				row.status === 'changed' &&
				stored?.contentHash &&
				deps.getBody &&
				bodyHash(await deps.getBody(row.converted.slug)) !== stored.contentHash
			) {
				const { [key]: _dropped, ...rest } = index;
				index = rest;
				results.push({
					skillKey: key,
					ok: false,
					error: t('agents.helpers.skillImport.flow.replaced'),
				});
				continue;
			}
			let fileSet: SkillFileSet = { files: [], sha256: EMPTY_FILES_SHA256 };
			if (uploadableFiles(row.skill).length > 0) {
				const [read] = (await deps.readFiles?.([key])) ?? [];
				if (!read?.ok || read.files_sha256 !== row.skill.files_sha256) {
					results.push({
						skillKey: key,
						ok: false,
						error: read?.error || changedSincePreview(),
					});
					continue;
				}
				fileSet = { files: read.files, sha256: read.files_sha256 };
			}
			const saved = await deps.put(
				row.converted.slug,
				row.converted.markdown,
				fileSet,
				libraryFilesSha(library, row.converted.slug),
			);
			index = recordImport(index, key, {
				slug: row.converted.slug,
				sourceHash: row.skill.hash,
				libraryVersion: saved.version,
				importedAt: (deps.now ?? (() => new Date().toISOString()))(),
				contentHash: bodyHash(row.converted.markdown),
				filesSha256: fileSet.sha256,
			});
			results.push({ skillKey: key, ok: true });
		} catch (error: any) {
			const errors = error?.response?.data?.errors;
			const detail = errors
				? Object.values(errors).flat().join(' ')
				: error?.response?.data?.message;
			results.push({
				skillKey: key,
				ok: false,
				error:
					error?.response?.status === 409
						? filesChangedInLibrary()
						: detail || t('agents.helpers.skillImport.flow.requestFailed'),
			});
		}
	}
	return { results, index, blocked: plan.blocked };
};
