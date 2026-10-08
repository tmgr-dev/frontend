import type { PageType } from '@/actions/tmgr/pages';
import { parse, stringify } from 'yaml';

export interface Frontmatter {
	title?: string;
	type?: string;
	properties?: Record<string, any>;
	tmgr?: { workspace?: string; id?: number };
}

export interface ParsedFrontmatter {
	frontmatter: Frontmatter | null;
	body: string;
	ignoredKeys: string[];
	warning: string | null;
}

const KNOWN_KEYS = ['title', 'type', 'properties', 'tmgr'];
const OPENING = /^﻿?---[ \t]*\r?\n/;
const CLOSING = /^(?:---|\.\.\.)[ \t]*$/;

const isObject = (value: unknown): value is Record<string, any> =>
	!!value && typeof value === 'object' && !Array.isArray(value);

const splitBlock = (text: string): { yaml: string; body: string } | null => {
	const opening = OPENING.exec(text);
	if (!opening) return null;
	const rest = text.slice(opening[0].length);
	const lines = rest.split('\n');
	let offset = 0;
	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index].replace(/\r$/, '');
		if (CLOSING.test(line)) {
			return {
				yaml: rest.slice(0, offset),
				body: rest
					.slice(offset + lines[index].length + 1)
					.replace(/^\r?\n/, ''),
			};
		}
		offset += lines[index].length + 1;
	}
	return null;
};

const none = (text: string, warning: string | null): ParsedFrontmatter => ({
	frontmatter: null,
	body: text,
	ignoredKeys: [],
	warning,
});

export const parseFrontmatter = (text: string): ParsedFrontmatter => {
	const block = splitBlock(text);
	if (!block) return none(text, null);
	let data: unknown;
	try {
		data = parse(block.yaml, { schema: 'core', maxAliasCount: 20 });
	} catch {
		return none(text, 'Frontmatter is not valid YAML and was left in the page');
	}
	if (data === null || data === undefined) {
		return {
			frontmatter: {},
			body: block.body,
			ignoredKeys: [],
			warning: null,
		};
	}
	if (!isObject(data)) {
		return none(text, 'Frontmatter is not a mapping and was left in the page');
	}
	const frontmatter: Frontmatter = {};
	if (typeof data.title === 'string' && data.title.trim()) {
		frontmatter.title = data.title.trim();
	} else if (typeof data.title === 'number') {
		frontmatter.title = String(data.title);
	}
	if (typeof data.type === 'string') frontmatter.type = data.type.trim();
	if (isObject(data.properties)) frontmatter.properties = data.properties;
	if (isObject(data.tmgr)) {
		const { workspace, id } = data.tmgr;
		frontmatter.tmgr = {};
		if (typeof workspace === 'string') frontmatter.tmgr.workspace = workspace;
		if (Number.isInteger(id) && id > 0) frontmatter.tmgr.id = id;
	}
	return {
		frontmatter,
		body: block.body,
		ignoredKeys: Object.keys(data).filter((key) => !KNOWN_KEYS.includes(key)),
		warning: null,
	};
};

export interface ExportFrontmatter {
	title: string;
	type: PageType;
	properties?: Record<string, any> | null;
	tmgr: { workspace: string; id: number };
}

export const serializeFrontmatter = (data: ExportFrontmatter): string => {
	const document: Record<string, any> = {
		title: data.title,
		type: data.type,
	};
	if (
		(data.type === 'person' || data.type === 'meeting') &&
		data.properties &&
		Object.keys(data.properties).length
	) {
		document.properties = data.properties;
	}
	document.tmgr = data.tmgr;
	return `---\n${stringify(document, { schema: 'core', lineWidth: 0 })}---\n\n`;
};
