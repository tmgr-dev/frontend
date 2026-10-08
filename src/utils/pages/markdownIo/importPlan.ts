import type { PageType } from '@/actions/tmgr/pages';
import { parseFrontmatter } from './frontmatter';
import {
	collectLinks,
	isExternalTarget,
	isTmgrTarget,
	splitTargetFragment,
	type LinkToken,
} from './links';
import {
	baseName,
	decodePath,
	dirName,
	extName,
	resolveRelative,
	safeEntryPath,
	stripExt,
} from './paths';
import {
	IMPORT_LIMITS,
	type ImportLimits,
	type ImportPlan,
	type IoWarning,
	type PlannedPage,
	type VirtualFile,
} from './types';

const MARKDOWN_EXT = /\.(md|markdown)$/i;
const NOTION_SUFFIX = / [0-9a-f]{32}$/i;
const MAX_TITLE = 255;
const TYPES: PageType[] = ['plain', 'person', 'meeting'];

const lower = (value: string): string => value.toLowerCase();

const cleanName = (name: string): string =>
	name.replace(NOTION_SUFFIX, '').trim();

export interface LinkIndex {
	pagesByPath: Map<string, string>;
	pagesByName: Map<string, string>;
	filesByPath: Map<string, string>;
	filesByName: Map<string, string>;
}

export type LinkResolution =
	| { type: 'skip' }
	| { type: 'missing'; reportable: boolean }
	| { type: 'page'; key: string; fragment: string }
	| { type: 'file'; path: string; image: boolean };

export const buildLinkIndex = (
	pages: PlannedPage[],
	files: Record<string, VirtualFile>,
): LinkIndex => {
	const index: LinkIndex = {
		pagesByPath: new Map(),
		pagesByName: new Map(),
		filesByPath: new Map(),
		filesByName: new Map(),
	};
	for (const page of pages) {
		if (page.path) index.pagesByPath.set(lower(page.path), page.key);
	}
	for (const page of pages) {
		if (!page.path) continue;
		const name = lower(cleanName(stripExt(baseName(page.path))));
		if (!index.pagesByName.has(name)) index.pagesByName.set(name, page.key);
	}
	for (const page of pages) {
		const name = lower(page.title.trim());
		if (!index.pagesByName.has(name)) index.pagesByName.set(name, page.key);
	}
	for (const path of Object.keys(files)) {
		index.filesByPath.set(lower(path), path);
		const name = lower(baseName(path));
		if (!index.filesByName.has(name)) index.filesByName.set(name, path);
	}
	return index;
};

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;

const resolveWikilink = (
	index: LinkIndex,
	fromPath: string | null,
	token: LinkToken,
): LinkResolution => {
	const hash = token.target.indexOf('#');
	const name = (
		hash === -1 ? token.target : token.target.slice(0, hash)
	).trim();
	const fragment = hash === -1 ? '' : token.target.slice(hash);
	if (!name) return { type: 'skip' };
	const ext = extName(name);
	if (ext && !MARKDOWN_EXT.test(name)) {
		const path = safeEntryPath(name);
		const found =
			(path && index.filesByPath.get(lower(path))) ||
			(fromPath !== null &&
				(() => {
					const near = resolveRelative(fromPath, name);
					return near ? index.filesByPath.get(lower(near)) : undefined;
				})()) ||
			index.filesByName.get(lower(baseName(name)));
		return found
			? { type: 'file', path: found, image: IMAGE_EXT.test(found) }
			: { type: 'missing', reportable: true };
	}
	const pageName = name.replace(MARKDOWN_EXT, '');
	const path = safeEntryPath(`${pageName}.md`);
	const key =
		(path && index.pagesByPath.get(lower(path))) ||
		index.pagesByName.get(lower(baseName(pageName)));
	return key
		? { type: 'page', key, fragment }
		: { type: 'missing', reportable: true };
};

export const resolveLink = (
	index: LinkIndex,
	fromPath: string | null,
	token: LinkToken,
): LinkResolution => {
	if (token.kind === 'wikilink' || token.kind === 'embed') {
		return resolveWikilink(index, fromPath, token);
	}
	const target = token.target.trim();
	if (!target || target.startsWith('#')) return { type: 'skip' };
	if (isTmgrTarget(target) || isExternalTarget(target)) return { type: 'skip' };
	const { base, fragment } = splitTargetFragment(target);
	const query = base.indexOf('?');
	const decoded = decodePath(query === -1 ? base : base.slice(0, query));
	const path = resolveRelative(fromPath ?? '', decoded);
	const isMarkdown = !!path && MARKDOWN_EXT.test(path);
	const reportable = isMarkdown || token.kind === 'image';
	if (!path) return { type: 'missing', reportable };
	const pageKey = isMarkdown
		? index.pagesByPath.get(lower(path))
		: extName(path)
		? undefined
		: index.pagesByPath.get(lower(`${path}.md`));
	if (pageKey) return { type: 'page', key: pageKey, fragment };
	const file = isMarkdown ? undefined : index.filesByPath.get(lower(path));
	if (file) return { type: 'file', path: file, image: IMAGE_EXT.test(file) };
	return { type: 'missing', reportable };
};

interface Draft {
	key: string;
	path: string | null;
	sortPath: string;
	folder: string | null;
	container: string;
	name: string;
	bytes: Uint8Array | null;
}

const decoder = new TextDecoder('utf-8');
const encoder = new TextEncoder();

const byteLength = (text: string): number => encoder.encode(text).length;

const splitHeading = (body: string): { title: string | null; body: string } => {
	const lines = body.split('\n');
	const first = lines.findIndex((line) => line.trim() !== '');
	if (first === -1) return { title: null, body };
	const match = /^#[ \t]+(.+?)(?:[ \t]+#+)?[ \t]*$/.exec(lines[first]);
	if (!match) return { title: null, body };
	const rest = lines
		.slice(first + 1)
		.join('\n')
		.replace(/^\n+/, '');
	return { title: match[1].trim(), body: rest };
};

const natural = (a: string, b: string): number =>
	a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });

export const buildImportPlan = (
	input: VirtualFile[],
	target: { existingTitles: string[] },
	limits: ImportLimits = IMPORT_LIMITS,
): ImportPlan => {
	const warnings: IoWarning[] = [];
	const markdown = input.filter((file) => MARKDOWN_EXT.test(file.path));
	const others = input.filter((file) => !MARKDOWN_EXT.test(file.path));
	if (!markdown.length) {
		warnings.push({ path: null, message: 'No Markdown files were found' });
		return { pages: [], files: {}, warnings };
	}

	const mdByLowerPath = new Map(markdown.map((f) => [lower(f.path), f]));
	const dirs = new Set<string>();
	for (const file of markdown) {
		let dir = dirName(file.path);
		while (dir && !dirs.has(dir)) {
			dirs.add(dir);
			dir = dirName(dir);
		}
	}

	const drafts: Draft[] = [];
	const folderDraft = new Map<string, Draft>();
	const folderFile = (dir: string): VirtualFile | undefined => {
		const parent = dirName(dir);
		const prefix = parent ? `${parent}/` : '';
		const name = baseName(dir);
		const candidates = [
			`${prefix}${name}.md`,
			`${prefix}${name}.markdown`,
			`${dir}/index.md`,
			`${dir}/README.md`,
			`${dir}/index.markdown`,
			`${dir}/readme.markdown`,
		];
		for (const candidate of candidates) {
			const found = mdByLowerPath.get(lower(candidate));
			if (found) return found;
		}
		return undefined;
	};

	const folderOfFile = new Map<string, string>();
	for (const dir of dirs) {
		const file = folderFile(dir);
		if (file) folderOfFile.set(file.path, dir);
	}

	for (const file of markdown) {
		const folder = folderOfFile.get(file.path) ?? null;
		drafts.push({
			key: '',
			path: file.path,
			sortPath: file.path,
			folder,
			container: folder ? dirName(folder) : dirName(file.path),
			name: cleanName(stripExt(baseName(folder ?? file.path))),
			bytes: file.bytes,
		});
	}
	const hasFolderFile = new Set(
		drafts.filter((d) => d.folder).map((d) => d.folder as string),
	);
	for (const dir of dirs) {
		if (hasFolderFile.has(dir)) continue;
		drafts.push({
			key: '',
			path: null,
			sortPath: dir,
			folder: dir,
			container: dirName(dir),
			name: cleanName(baseName(dir)),
			bytes: null,
		});
	}
	for (const draft of drafts) {
		if (draft.folder) folderDraft.set(draft.folder, draft);
	}

	const parentOf = new Map<Draft, Draft | null>();
	for (const draft of drafts) {
		parentOf.set(
			draft,
			draft.container ? folderDraft.get(draft.container) ?? null : null,
		);
	}
	const childrenOf = new Map<Draft | null, Draft[]>();
	for (const draft of drafts) {
		const parent = parentOf.get(draft) ?? null;
		childrenOf.set(parent, [...(childrenOf.get(parent) ?? []), draft]);
	}
	const ordered: Draft[] = [];
	const visit = (parent: Draft | null) => {
		const children = (childrenOf.get(parent) ?? []).sort((a, b) =>
			natural(a.sortPath, b.sortPath),
		);
		for (const child of children) {
			child.key = `p${ordered.length + 1}`;
			ordered.push(child);
			visit(child);
		}
	};
	visit(null);

	const pages: PlannedPage[] = [];
	const ignoredKeys = new Set<string>();
	const keptKey = new Map<Draft, string | null>();
	const keyOf = (draft: Draft | null): string | null =>
		draft === null ? null : keptKey.get(draft) ?? null;

	for (const draft of ordered) {
		let body = '';
		let title: string | null = null;
		let type: PageType = 'plain';
		let properties: Record<string, any> | null = null;
		let sourceId: number | null = null;
		let sourceWorkspace: string | null = null;
		if (draft.bytes) {
			const text = decoder
				.decode(draft.bytes)
				.replace(/^﻿/, '')
				.replace(/\r\n?/g, '\n');
			const parsed = parseFrontmatter(text);
			if (parsed.warning)
				warnings.push({ path: draft.path, message: parsed.warning });
			parsed.ignoredKeys.forEach((key) => ignoredKeys.add(key));
			const fm = parsed.frontmatter;
			body = parsed.body;
			title = fm?.title ?? null;
			if (!title) {
				const heading = splitHeading(body);
				title = heading.title;
				body = heading.body;
			}
			const declared = fm?.type?.toLowerCase();
			if (declared === 'context') {
				warnings.push({
					path: draft.path,
					message:
						'Page type "context" is reserved and was imported as a plain page',
				});
			} else if (declared && TYPES.includes(declared as PageType)) {
				type = declared as PageType;
			}
			if (type !== 'plain' && fm?.properties) properties = fm.properties;
			sourceId = fm?.tmgr?.id ?? null;
			sourceWorkspace = fm?.tmgr?.workspace ?? null;
		}
		title = (title || draft.name || 'Untitled').trim() || 'Untitled';
		if (title.length > MAX_TITLE) {
			title = title.slice(0, MAX_TITLE);
			warnings.push({
				path: draft.path,
				message: 'The title was shortened to 255 characters',
			});
		}
		if (byteLength(body) > limits.maxBodyBytes) {
			warnings.push({
				path: draft.path,
				message: `Skipped: the page is larger than ${Math.round(
					limits.maxBodyBytes / 1024,
				)} KB`,
			});
			keptKey.set(draft, null);
			continue;
		}
		keptKey.set(draft, draft.key);
		let ancestor = parentOf.get(draft) ?? null;
		while (ancestor && !keyOf(ancestor))
			ancestor = parentOf.get(ancestor) ?? null;
		pages.push({
			key: draft.key,
			path: draft.path,
			parentKey: keyOf(ancestor),
			title,
			type,
			properties,
			body,
			sourceId,
			sourceWorkspace,
			conflict: false,
		});
	}

	if (ignoredKeys.size) {
		warnings.push({
			path: null,
			message: `Ignored frontmatter keys: ${[...ignoredKeys]
				.sort()
				.join(', ')}`,
		});
	}

	const allFiles: Record<string, VirtualFile> = {};
	for (const file of others) allFiles[file.path] = file;
	const index = buildLinkIndex(pages, allFiles);
	const referenced = new Set<string>();
	for (const page of pages) {
		for (const token of collectLinks(page.body)) {
			const resolved = resolveLink(index, page.path, token);
			if (resolved.type === 'file') referenced.add(resolved.path);
			if (resolved.type === 'missing' && resolved.reportable) {
				warnings.push({
					path: page.path,
					message: `Link target not found: ${token.target}`,
				});
			}
		}
	}
	const files: Record<string, VirtualFile> = {};
	for (const file of others) {
		if (referenced.has(file.path)) {
			files[file.path] = file;
		} else {
			warnings.push({
				path: file.path,
				message: 'Skipped: no page refers to this file',
			});
		}
	}

	const existing = new Set(target.existingTitles.map((t) => lower(t.trim())));
	for (const page of pages) {
		if (page.parentKey === null && existing.has(lower(page.title.trim()))) {
			page.conflict = true;
		}
	}
	return { pages, files, warnings };
};
