import {
	convertSkill,
	defaultTicked,
	planImport,
	recordImport,
	rowStatus,
	suggestSlug,
	textHash,
	type ConvertedSkill,
	type ImportIndex,
	type LibraryEntryRef,
	type RowStatus,
	type ScanResult,
	type ScanSkill,
} from '@/utils/skillImport';

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
						base.bodyMatches = sameMarkdown(
							await fetchBody(converted.slug),
							converted.markdown,
						);
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
			});
		}
	}
	return next;
};

export const clashSuggestion = (row: ImportRow): string =>
	suggestSlug(row.packageName, row.converted.slug);

export interface ImportDeps {
	listLibrary: () => Promise<LibraryEntryRef[]>;
	put: (slug: string, markdown: string) => Promise<{ version: number }>;
	getBody?: (slug: string) => Promise<string>;
	index: ImportIndex;
	now?: () => string;
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
					error:
						'The library skill was replaced since the last import. Tick it again to overwrite.',
				});
				continue;
			}
			const saved = await deps.put(row.converted.slug, row.converted.markdown);
			index = recordImport(index, key, {
				slug: row.converted.slug,
				sourceHash: row.skill.hash,
				libraryVersion: saved.version,
				importedAt: (deps.now ?? (() => new Date().toISOString()))(),
				contentHash: bodyHash(row.converted.markdown),
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
				error: detail || 'Request failed',
			});
		}
	}
	return { results, index, blocked: plan.blocked };
};
