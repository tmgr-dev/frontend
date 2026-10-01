export type TmgrKind =
	| 'page'
	| 'task'
	| 'category'
	| 'user'
	| 'persona'
	| 'file';

export interface StorageRef {
	kind: TmgrKind;
	id: string;
}

export type ParsedTmgrUrl =
	| { form: 'storage'; kind: TmgrKind; id: string }
	| { form: 'deep'; kind: 'page'; workspace: string; slug: string };

const NUMERIC_KINDS = 'page|task|category|user|file';
const STORAGE_NUMERIC = new RegExp(`^tmgr://(${NUMERIC_KINDS})/(\\d+)$`);
const STORAGE_PERSONA = /^tmgr:\/\/persona\/([A-Za-z0-9-]+)$/;
const DEEP_PAGE = /^tmgr:\/\/page\/([A-Za-z0-9_-]+)\/([A-Za-z0-9_-]+)$/;

export const parseTmgrUrl = (url: string): ParsedTmgrUrl | null => {
	const value = (url || '').trim();
	const numeric = STORAGE_NUMERIC.exec(value);
	if (numeric) {
		return { form: 'storage', kind: numeric[1] as TmgrKind, id: numeric[2] };
	}
	const persona = STORAGE_PERSONA.exec(value);
	if (persona) return { form: 'storage', kind: 'persona', id: persona[1] };
	const deep = DEEP_PAGE.exec(value);
	if (deep) {
		return { form: 'deep', kind: 'page', workspace: deep[1], slug: deep[2] };
	}
	return null;
};

const stripCode = (markdown: string): string =>
	markdown
		.replace(/^ {0,3}(```|~~~)[\s\S]*?(?:^ {0,3}\1[^\n]*$|(?![\s\S]))/gm, '')
		.replace(/`[^`\n]*`/g, '');

export const extractTmgrRefs = (markdown: string): StorageRef[] => {
	const seen = new Set<string>();
	const refs: StorageRef[] = [];
	for (const match of stripCode(markdown).matchAll(
		/\]\((tmgr:\/\/[^)\s]+)\)/g,
	)) {
		const parsed = parseTmgrUrl(match[1]);
		if (!parsed || parsed.form !== 'storage') continue;
		const key = `${parsed.kind}/${parsed.id}`;
		if (seen.has(key)) continue;
		seen.add(key);
		refs.push({ kind: parsed.kind, id: parsed.id });
	}
	return refs;
};

export const buildTmgrLink = (
	kind: TmgrKind,
	id: number | string,
	title: string,
): string => `[${title.replace(/([[\]])/g, '\\$1')}](tmgr://${kind}/${id})`;

export interface PathContext {
	workspaceCode: string;
	pageSlugs: Record<string, string>;
	categoryCodes: Record<string, string>;
}

export const pathForRef = (
	ref: StorageRef | ParsedTmgrUrl,
	ctx: PathContext,
): string | null => {
	if ('workspace' in ref) return `/${ref.workspace}/pages/${ref.slug}`;
	const { kind, id } = ref;
	const ws = ctx.workspaceCode;
	switch (kind) {
		case 'page': {
			const slug = ctx.pageSlugs[id];
			return slug ? `/${ws}/pages/${slug}` : null;
		}
		case 'task':
			return `/${ws}/tasks/${id}`;
		case 'user':
			return `/${ws}/team/${id}`;
		case 'category': {
			const code = ctx.categoryCodes[id];
			return code ? `/${ws}/${code}` : `/${ws}/categories`;
		}
		case 'persona':
			return '/settings/personas';
		default:
			return null;
	}
};
