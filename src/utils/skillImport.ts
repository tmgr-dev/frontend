import {
	byteLength,
	SHARED_SKILL_LIMIT,
	SKILL_BODY_MAX_BYTES,
	SKILL_TITLE_MAX_LENGTH,
	validateSkillMarkdown,
} from '@/utils/personas';
import { parse } from 'yaml';

export interface ScanSupporting {
	name: string;
	size: number;
}

export interface ScanSkill {
	key: string;
	dir_name: string;
	path: string;
	content: string;
	hash: string;
	size: number;
	too_large: boolean;
	secret: boolean;
	secret_reason: string | null;
	symlink_target: string | null;
	supporting: ScanSupporting[];
	supporting_more: number;
}

export interface ScanPackage {
	id: string;
	name: string;
	version: string | null;
	skills: ScanSkill[];
}

export interface ScanSource {
	id: 'claude' | 'claude-plugin' | 'codex' | 'agents' | 'folder';
	label: string;
	packages: ScanPackage[];
}

export interface ScanResult {
	sources: ScanSource[];
	skipped: { path: string; reason: string }[];
}

export interface ConvertedSkill {
	slug: string;
	title: string;
	when: string;
	markdown: string;
	bytes: number;
	droppedFields: string[];
	warnings: string[];
	blockedReason: string | null;
}

const SLUG_MAX_LENGTH = 60;
const WHEN_MAX_LENGTH = 1024;
const WHEN_PREFIX = 'Use when: ';
const YAML_MAX_ALIASES = 10;
const CONSUMED_KEYS: ReadonlySet<string> = new Set(['name', 'description']);

const shortHash = (seed: string): string => {
	let hash = 0x811c9dc5;
	for (let i = 0; i < seed.length; i++) {
		hash ^= seed.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193);
	}
	return (hash >>> 0).toString(16).padStart(8, '0').slice(0, 6);
};

const toSlugPart = (text: string): string =>
	(text ?? '')
		.normalize('NFKD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, SLUG_MAX_LENGTH)
		.replace(/-+$/, '');

export const slugify = (
	name: string,
	dirName: string,
	hashSeed: string,
): string =>
	toSlugPart(name) || toSlugPart(dirName) || `skill-${shortHash(hashSeed)}`;

export const suggestSlug = (packageName: string, slug: string): string =>
	slugify(`${packageName}-${slug}`, slug, `${packageName}-${slug}`);

const trimLeadingBlank = (text: string): string =>
	text.replace(/^(\s*\n)+/, '');

const splitSource = (
	content: string,
): { frontMatter: string | null; body: string } => {
	const normalized = (content ?? '').replace(/^﻿/, '').replace(/\r\n/g, '\n');
	const lines = normalized.split('\n');
	if (lines[0]?.trim() !== '---') {
		return { frontMatter: null, body: trimLeadingBlank(normalized) };
	}
	const closing = lines.findIndex(
		(line, index) => index > 0 && line.trim() === '---',
	);
	if (closing < 0) {
		return { frontMatter: null, body: trimLeadingBlank(normalized) };
	}
	return {
		frontMatter: lines.slice(1, closing).join('\n'),
		body: trimLeadingBlank(lines.slice(closing + 1).join('\n')),
	};
};

const readFrontMatter = (
	raw: string | null,
): { data: Record<string, unknown>; invalid: boolean } => {
	if (raw === null) return { data: {}, invalid: false };
	try {
		const parsed: unknown = parse(raw, { maxAliasCount: YAML_MAX_ALIASES });
		if (parsed === null || parsed === undefined) {
			return { data: {}, invalid: false };
		}
		if (typeof parsed !== 'object' || Array.isArray(parsed)) {
			return { data: {}, invalid: true };
		}
		return { data: parsed as Record<string, unknown>, invalid: false };
	} catch {
		return { data: {}, invalid: true };
	}
};

const scalarText = (value: unknown): string =>
	typeof value === 'string' || typeof value === 'number'
		? String(value).trim()
		: '';

const collapse = (text: string): string => text.replace(/\s+/g, ' ').trim();

const firstParagraph = (body: string): string =>
	body
		.split(/\n\s*\n/)
		.map(collapse)
		.find((paragraph) => paragraph && !/^(#|```|~~~)/.test(paragraph)) ?? '';

const stripMarkdown = (text: string): string =>
	collapse(
		text
			.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
			.replace(/[`*~]+/g, '')
			.replace(/(^|\s)_+|_+(?=\s|$)/g, '$1'),
	);

const FENCE = /^\s*(```|~~~)/;

const firstHeading = (body: string): string => {
	let inFence = false;
	for (const line of body.split('\n')) {
		if (FENCE.test(line)) {
			inFence = !inFence;
			continue;
		}
		if (inFence) continue;
		const match = /^# +(.+)$/.exec(line);
		const heading = match ? stripMarkdown(match[1]) : '';
		if (heading) return heading;
	}
	return '';
};

const humanise = (name: string): string => {
	const text = name.replace(/[-_]+/g, ' ').trim();
	return text.charAt(0).toUpperCase() + text.slice(1);
};

const repairWhen = (text: string): string =>
	/^(---|\[)/.test(text) ? `${WHEN_PREFIX}${text}` : text;

const fitWhen = (text: string, warnings: string[]): string => {
	if (text.length <= WHEN_MAX_LENGTH) return text;
	warnings.push(`description cut to ${WHEN_MAX_LENGTH} characters`);
	return text.slice(0, WHEN_MAX_LENGTH).trimEnd();
};

const bodyWarnings = (body: string): string[] => {
	const warnings: string[] = [];
	if (body.includes('CLAUDE_PLUGIN_ROOT')) {
		warnings.push(
			'refers to ${CLAUDE_PLUGIN_ROOT}, which does not exist in the library',
		);
	}
	if (/\]\(\.{1,2}\//.test(body) || /(^|[\s`'"(])scripts\//m.test(body)) {
		warnings.push('refers to relative paths that will not resolve');
	}
	return warnings;
};

const blockedReasonOf = (
	skill: ScanSkill,
	markdown: string,
	bytes: number,
	slug: string,
	when: string,
): string | null => {
	if (skill.secret) return 'looks like it contains a secret';
	if (skill.too_large || bytes > SKILL_BODY_MAX_BYTES) {
		return `${Math.ceil(bytes / 1024)} KiB, limit ${SKILL_BODY_MAX_BYTES / 1024} KiB`;
	}
	if (!skill.content.trim()) return 'empty file';
	if (!when) return 'no description';
	const errors = validateSkillMarkdown(markdown, slug);
	if (!errors) return null;
	return Object.entries(errors)
		.map(([field, messages]) => `${field} ${messages.join(', ')}`)
		.join('; ');
};

export const convertSkill = (
	skill: ScanSkill,
	slugOverride?: string,
): ConvertedSkill => {
	const { frontMatter, body } = splitSource(skill.content);
	const { data, invalid } = readFrontMatter(frontMatter);
	const name = scalarText(data.name);
	const description = collapse(scalarText(data.description));

	const warnings: string[] = [];
	if (frontMatter === null) warnings.push('no front matter');
	if (invalid) {
		warnings.push(
			'front matter is not valid YAML; name and description were not read',
		);
	}
	const supportingCount = skill.supporting.length + skill.supporting_more;
	if (supportingCount > 0) {
		warnings.push(
			`${supportingCount} supporting ${supportingCount === 1 ? 'file' : 'files'} not imported; relative references will not resolve`,
		);
	}
	warnings.push(...bodyWarnings(body));

	const slug = slugOverride?.trim() || slugify(name, skill.dir_name, skill.key);
	const title = (firstHeading(body) || humanise(name || skill.dir_name))
		.slice(0, SKILL_TITLE_MAX_LENGTH)
		.trim();
	const when = fitWhen(
		repairWhen(description || firstParagraph(body)),
		warnings,
	);
	const markdown = `---\nslug: ${slug}\ntitle: ${title}\nwhen: ${when}\nactions: []\n---\n${body}`;
	const bytes = skill.too_large ? skill.size : byteLength(markdown);

	return {
		slug,
		title,
		when,
		markdown,
		bytes,
		droppedFields: Object.keys(data).filter((key) => !CONSUMED_KEYS.has(key)),
		warnings,
		blockedReason: blockedReasonOf(skill, markdown, bytes, slug, when),
	};
};

export interface IndexEntry {
	slug: string;
	sourceHash: string;
	libraryVersion: number;
	importedAt: string;
}

export type ImportIndex = Record<string, IndexEntry>;

export interface KeyValueStorage {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

export const indexStorageKey = (
	apiBase: string,
	userId: number | string,
): string => `tmgr.skillImport.v1:${apiBase}:${userId}`;

const isIndexEntry = (value: unknown): value is IndexEntry => {
	if (!value || typeof value !== 'object') return false;
	const entry = value as Record<string, unknown>;
	return (
		typeof entry.slug === 'string' &&
		typeof entry.sourceHash === 'string' &&
		typeof entry.libraryVersion === 'number' &&
		typeof entry.importedAt === 'string'
	);
};

export const loadIndex = (
	storage: KeyValueStorage,
	key: string,
): ImportIndex => {
	try {
		const parsed: unknown = JSON.parse(storage.getItem(key) ?? 'null');
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
			return {};
		}
		const entries = Object.entries(parsed as Record<string, unknown>).filter(
			(pair): pair is [string, IndexEntry] => isIndexEntry(pair[1]),
		);
		return Object.fromEntries(entries);
	} catch {
		return {};
	}
};

export const saveIndex = (
	storage: KeyValueStorage,
	key: string,
	index: ImportIndex,
): boolean => {
	try {
		storage.setItem(key, JSON.stringify(index));
		return true;
	} catch {
		return false;
	}
};

export const recordImport = (
	index: ImportIndex,
	skillKey: string,
	entry: IndexEntry,
): ImportIndex => ({ ...index, [skillKey]: entry });

export type RowStatus =
	| 'new'
	| 'up-to-date'
	| 'changed'
	| 'edited-in-library'
	| 'slug-taken'
	| 'previously-imported-deleted'
	| 'blocked';

export interface LibraryEntryRef {
	slug: string;
	version: number;
}

export interface RowStatusInput {
	skill: ScanSkill;
	converted: ConvertedSkill;
	entry?: IndexEntry;
	library: LibraryEntryRef[];
	libraryBodyMatches?: boolean;
}

export const rowStatus = ({
	skill,
	converted,
	entry,
	library,
	libraryBodyMatches,
}: RowStatusInput): RowStatus => {
	if (converted.blockedReason) return 'blocked';
	if (entry) {
		const inLibrary = library.find((item) => item.slug === entry.slug);
		if (!inLibrary) return 'previously-imported-deleted';
		if (inLibrary.version !== entry.libraryVersion) return 'edited-in-library';
		return skill.hash === entry.sourceHash ? 'up-to-date' : 'changed';
	}
	if (!library.some((item) => item.slug === converted.slug)) return 'new';
	return libraryBodyMatches ? 'up-to-date' : 'slug-taken';
};

const DEFAULT_TICKED: ReadonlySet<RowStatus> = new Set<RowStatus>([
	'new',
	'changed',
	'previously-imported-deleted',
]);

export const defaultTicked = (status: RowStatus): boolean =>
	DEFAULT_TICKED.has(status);

export interface PlanItem {
	skillKey: string;
	slug: string;
	status: RowStatus;
	ticked: boolean;
}

export interface ImportPlan {
	toImport: string[];
	blocked: { skillKey: string; reason: string }[];
}

export const planImport = (
	items: PlanItem[],
	librarySlugs: string[],
	limit = SHARED_SKILL_LIMIT,
): ImportPlan => {
	const inLibrary = new Set(librarySlugs);
	const usedSlugs = new Set<string>();
	const toImport: string[] = [];
	const blocked: ImportPlan['blocked'] = [];
	let projectedCount = librarySlugs.length;

	for (const item of items) {
		if (!item.ticked) continue;
		if (item.status === 'blocked' || item.status === 'up-to-date') continue;
		if (usedSlugs.has(item.slug)) {
			blocked.push({
				skillKey: item.skillKey,
				reason: 'duplicate slug in selection',
			});
			continue;
		}
		usedSlugs.add(item.slug);
		if (!inLibrary.has(item.slug)) {
			if (projectedCount + 1 > limit) {
				blocked.push({
					skillKey: item.skillKey,
					reason: 'library limit reached',
				});
				continue;
			}
			projectedCount += 1;
		}
		toImport.push(item.skillKey);
	}

	return { toImport, blocked };
};

export const importSummary = (results: { ok: boolean }[]): string => {
	const ok = results.filter((result) => result.ok).length;
	const failed = results.length - ok;
	const imported = `Imported ${ok} ${ok === 1 ? 'skill' : 'skills'}`;
	return failed === 0 ? imported : `${imported}, ${failed} failed`;
};
