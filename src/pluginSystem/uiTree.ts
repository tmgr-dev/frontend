import { Marked, type Token } from 'marked';
import { PLUGIN_API_VERSION } from './manifest';

export type Tone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
export type Color =
	'gray' | 'green' | 'yellow' | 'red' | 'blue' | 'purple' | 'orange';

export type Gap = 'none' | 'sm' | 'md' | 'lg';
export type Command = { command: string; args?: unknown };
export type MenuItem = {
	text: string;
	command: string;
	args?: unknown;
	confirm?: string;
};

export type MdInline =
	| { type: 'text'; text: string }
	| { type: 'strong' | 'em' | 'del'; children: MdInline[] }
	| { type: 'codespan'; text: string }
	| { type: 'br' }
	| { type: 'link'; url: string; host: string; children: MdInline[] };

export type MdBlock =
	| { type: 'heading'; depth: 1 | 2 | 3; children: MdInline[] }
	| { type: 'paragraph'; children: MdInline[] }
	| {
			type: 'list';
			ordered: boolean;
			start: number;
			items: { checked?: boolean; blocks: MdBlock[] }[];
	  }
	| {
			type: 'table';
			header: MdInline[][];
			align: ('left' | 'center' | 'right' | null)[];
			rows: MdInline[][][];
	  }
	| { type: 'code'; text: string; lang?: string }
	| { type: 'blockquote'; blocks: MdBlock[] }
	| { type: 'hr' };

export type TextSpan = {
	text: string;
	bold?: true;
	italic?: true;
	code?: true;
	strike?: true;
};

export type UiNode =
	| {
			type: 'stack';
			direction: 'row' | 'column';
			children: UiNode[];
			gap?: Gap;
			align?: 'start' | 'center' | 'end' | 'stretch';
			justify?: 'start' | 'between' | 'end';
			grow?: boolean;
	  }
	| {
			type: 'card';
			children: UiNode[];
			tone: 'default' | 'muted' | 'raised';
			padding: 'sm' | 'md' | 'lg';
			accent?: Color;
			onClick?: Command;
			label?: string;
			grow?: boolean;
	  }
	| {
			type: 'grid';
			columns: number;
			minWidth: number;
			gap: 'sm' | 'md' | 'lg';
			children: UiNode[];
	  }
	| { type: 'menu'; label?: string; icon: 'more'; items: MenuItem[] }
	| { type: 'heading'; text: string; level: 1 | 2 | 3 }
	| {
			type: 'text';
			text: string;
			tone: Tone;
			spans?: TextSpan[];
			breaks?: true;
	  }
	| { type: 'markdown'; blocks: MdBlock[] }
	| { type: 'code'; text: string; lang?: string; wrap: boolean }
	| {
			type: 'badge';
			text: string;
			color: Color;
			command?: string;
			args?: unknown;
	  }
	| {
			type: 'stat';
			label: string;
			value: string;
			tone: Tone;
			command?: string;
			args?: unknown;
	  }
	| { type: 'progress'; value: number; color: Color }
	| { type: 'list'; items: UiNode[] }
	| {
			type: 'table';
			columns: { key: string; title: string }[];
			rows: { taskId?: number; cells: Record<string, UiNode> }[];
	  }
	| {
			type: 'button';
			text: string;
			command: string;
			args?: unknown;
			confirm?: string;
			variant?: 'default' | 'primary' | 'ghost';
			size?: 'sm' | 'md';
	  }
	| { type: 'taskLink'; taskId: number; text: string }
	| { type: 'divider' }
	| { type: 'copyable'; text: string; label?: string }
	| { type: 'link'; url: string; text: string; host: string }
	| { type: 'timeAgo'; at: string }
	| { type: 'dueTime'; at: string }
	| { type: 'keyValue'; items: { key: string; value: UiNode }[] };

const TONES: Tone[] = ['default', 'muted', 'success', 'warning', 'danger'];
export const COLORS: Color[] = [
	'gray',
	'green',
	'yellow',
	'red',
	'blue',
	'purple',
	'orange',
];
const MAX_DEPTH = 8;
const MAX_NODES = 3000;
const MAX_ITEMS = 500;
const MAX_TEXT = 1000;
const MAX_LONG_TEXT = 20000;
const MAX_LONG_TEXT_TOTAL = 200000;
const MAX_SPANS = 200;
const MAX_LANG = 30;
const MAX_MD_DEPTH = 6;
const MAX_MD_COLUMNS = 12;
const MAX_MD_ROWS = 200;
const SPAN_FLAGS = [
	['bold', 'bold'],
	['italic', 'italic'],
	['code', 'code'],
	['strike', 'strike'],
] as const;
const MAX_COPYABLE_TEXT = 2000;
const MAX_KEY_VALUE_ITEMS = 50;
const MAX_BUTTON_ARGS_BYTES = 8 * 1024;
const MAX_AT_LENGTH = 64;
const MAX_CARD_DEPTH = 3;
const MAX_MENU_ITEMS = 20;
const MAX_LABEL_LENGTH = 200;
const CURRENT_API_MINOR = Number(PLUGIN_API_VERSION.split('.')[1]);

const GAPS: Gap[] = ['none', 'sm', 'md', 'lg'];
const GRID_GAPS: ('sm' | 'md' | 'lg')[] = ['sm', 'md', 'lg'];
const ALIGNS: ('start' | 'center' | 'end' | 'stretch')[] = [
	'start',
	'center',
	'end',
	'stretch',
];
const JUSTIFIES: ('start' | 'between' | 'end')[] = ['start', 'between', 'end'];
const CARD_TONES: ('default' | 'muted' | 'raised')[] = [
	'default',
	'muted',
	'raised',
];
const PADDINGS: ('sm' | 'md' | 'lg')[] = ['sm', 'md', 'lg'];
const BUTTON_VARIANTS: ('default' | 'primary' | 'ghost')[] = [
	'default',
	'primary',
	'ghost',
];
const BUTTON_SIZES: ('sm' | 'md')[] = ['sm', 'md'];

/** What a plugin's manifest allows a `link` node to open; the sanitiser stays pure by taking this in. */
export interface LinkContext {
	allowedDomains: string[];
	linksOpen: boolean;
}

export const NO_LINKS: LinkContext = { allowedDomains: [], linksOpen: false };

/** https only, and only a domain the plugin declared and has permission for. Shared by sanitizeTree and host.openLink. */
export const isLinkAllowed = (url: string, context: LinkContext): boolean => {
	if (!context.linksOpen) return false;
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		return false;
	}
	return (
		parsed.protocol === 'https:' &&
		context.allowedDomains.includes(parsed.hostname)
	);
};

const NAMED_ENTITIES: Record<string, string> = {
	amp: '&',
	lt: '<',
	gt: '>',
	quot: '"',
	apos: "'",
	nbsp: ' ',
};

const decodeEntities = (value: string): string =>
	value.replace(
		/&(#x[0-9a-f]{1,6}|#[0-9]{1,7}|amp|lt|gt|quot|apos|nbsp);/gi,
		(match, entity: string) => {
			const lower = entity.toLowerCase();
			if (lower[0] !== '#') return NAMED_ENTITIES[lower];
			const code =
				lower[1] === 'x'
					? parseInt(lower.slice(2), 16)
					: parseInt(lower.slice(1), 10);
			return code > 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff)
				? String.fromCodePoint(code)
				: match;
		},
	);

const markdownLexer = new Marked({ gfm: true, breaks: true });

type MdContext = { take: () => boolean; links: LinkContext };

const flattenRaw = (tokens: Token[]): string =>
	tokens
		.map((token) => token.raw)
		.join('')
		.slice(0, MAX_TEXT);

const convertInline = (
	tokens: Token[] | undefined,
	ctx: MdContext,
	depth: number,
): MdInline[] => {
	if (!tokens) return [];
	if (depth > MAX_MD_DEPTH) {
		const plain = flattenRaw(tokens);
		return plain && ctx.take() ? [{ type: 'text', text: plain }] : [];
	}
	const out: MdInline[] = [];
	const push = (node: MdInline): boolean => {
		if (!ctx.take()) return false;
		out.push(node);
		return true;
	};
	for (const token of tokens) {
		const t = token as any;
		switch (token.type) {
			case 'text':
				if (Array.isArray(t.tokens) && t.tokens.length > 0) {
					out.push(...convertInline(t.tokens, ctx, depth + 1));
				} else if (!push({ type: 'text', text: decodeEntities(t.text ?? '') }))
					return out;
				break;
			case 'escape':
				if (!push({ type: 'text', text: String(t.text ?? '') })) return out;
				break;
			case 'html':
				if (!push({ type: 'text', text: String(t.raw ?? '') })) return out;
				break;
			case 'image':
				if (!push({ type: 'text', text: decodeEntities(String(t.text ?? '')) }))
					return out;
				break;
			case 'strong':
			case 'em':
			case 'del': {
				if (!ctx.take()) return out;
				out.push({
					type: token.type,
					children: convertInline(t.tokens, ctx, depth + 1),
				});
				break;
			}
			case 'codespan':
				if (!push({ type: 'codespan', text: String(t.text ?? '') })) return out;
				break;
			case 'br':
				if (!push({ type: 'br' })) return out;
				break;
			case 'link': {
				const href = typeof t.href === 'string' ? t.href.slice(0, 2000) : '';
				const children = convertInline(t.tokens, ctx, depth + 1);
				let host: string | null = null;
				if (isLinkAllowed(href, ctx.links)) {
					try {
						host = new URL(href).hostname;
					} catch {
						host = null;
					}
				}
				if (host === null) {
					out.push(...children);
				} else if (!push({ type: 'link', url: href, host, children })) {
					return out;
				}
				break;
			}
			default:
				break;
		}
	}
	return out;
};

const convertBlocks = (
	tokens: Token[],
	ctx: MdContext,
	depth: number,
): MdBlock[] => {
	const out: MdBlock[] = [];
	const push = (block: MdBlock): boolean => {
		if (!ctx.take()) return false;
		out.push(block);
		return true;
	};
	for (const token of tokens) {
		const t = token as any;
		if (depth > MAX_MD_DEPTH) {
			if (token.type === 'space' || token.type === 'def') continue;
			const plain = flattenRaw([token]).trim();
			if (
				plain &&
				!push({ type: 'paragraph', children: [{ type: 'text', text: plain }] })
			)
				return out;
			continue;
		}
		switch (token.type) {
			case 'heading': {
				const level = Math.min(3, Math.max(1, Number(t.depth) || 1)) as
					1 | 2 | 3;
				if (
					!push({
						type: 'heading',
						depth: level,
						children: convertInline(t.tokens, ctx, 0),
					})
				)
					return out;
				break;
			}
			case 'paragraph':
			case 'text':
				if (
					!push({
						type: 'paragraph',
						children: Array.isArray(t.tokens)
							? convertInline(t.tokens, ctx, 0)
							: [{ type: 'text', text: decodeEntities(String(t.text ?? '')) }],
					})
				)
					return out;
				break;
			case 'html': {
				const plain = String(t.raw ?? '')
					.trim()
					.slice(0, MAX_TEXT);
				if (
					plain &&
					!push({
						type: 'paragraph',
						children: [{ type: 'text', text: plain }],
					})
				)
					return out;
				break;
			}
			case 'list': {
				if (!ctx.take()) return out;
				const items = (Array.isArray(t.items) ? t.items : [])
					.slice(0, MAX_ITEMS)
					.map((item: any) => ({
						...(item.task ? { checked: item.checked === true } : {}),
						blocks: convertBlocks(item.tokens ?? [], ctx, depth + 1),
					}));
				out.push({
					type: 'list',
					ordered: t.ordered === true,
					start:
						typeof t.start === 'number' && Number.isFinite(t.start)
							? t.start
							: 1,
					items,
				});
				break;
			}
			case 'table': {
				if (!ctx.take()) return out;
				const header = (t.header ?? [])
					.slice(0, MAX_MD_COLUMNS)
					.map((cell: any) => convertInline(cell.tokens, ctx, 0));
				const align = (t.align ?? [])
					.slice(0, header.length)
					.map((a: unknown) =>
						a === 'left' || a === 'center' || a === 'right' ? a : null,
					);
				const rows = (t.rows ?? [])
					.slice(0, MAX_MD_ROWS)
					.map((row: any[]) =>
						row
							.slice(0, header.length)
							.map((cell: any) => convertInline(cell.tokens, ctx, 0)),
					);
				out.push({ type: 'table', header, align, rows });
				break;
			}
			case 'code': {
				const lang = String(t.lang ?? '')
					.trim()
					.split(/\s+/)[0]
					.slice(0, MAX_LANG);
				if (
					!push({
						type: 'code',
						text: String(t.text ?? '').slice(0, MAX_LONG_TEXT),
						...(lang ? { lang } : {}),
					})
				)
					return out;
				break;
			}
			case 'blockquote':
				if (!ctx.take()) return out;
				out.push({
					type: 'blockquote',
					blocks: convertBlocks(t.tokens ?? [], ctx, depth + 1),
				});
				break;
			case 'hr':
				if (!push({ type: 'hr' })) return out;
				break;
			default:
				break;
		}
	}
	return out;
};

const pick = <T>(value: unknown, allowed: readonly T[], fallback: T): T =>
	allowed.includes(value as T) ? (value as T) : fallback;

const pickOptional = <T>(
	value: unknown,
	allowed: readonly T[],
): T | undefined => (allowed.includes(value as T) ? (value as T) : undefined);

const text = (value: unknown): string | null =>
	typeof value === 'string'
		? value.slice(0, MAX_TEXT)
		: typeof value === 'number' && Number.isFinite(value)
			? String(value)
			: null;

const parseCommandArgs = (
	value: any,
): { command: string; args?: unknown } | null => {
	const command =
		typeof value?.command === 'string' ? value.command.slice(0, 120) : null;
	if (!command) return null;
	let args: unknown;
	try {
		if (value.args !== undefined) {
			const json = JSON.stringify(value.args);
			if (json.length > MAX_BUTTON_ARGS_BYTES) return null;
			args = JSON.parse(json);
		}
	} catch {
		return null;
	}
	return { command, ...(args !== undefined ? { args } : {}) };
};

const findLabel = (nodes: UiNode[]): string | null => {
	for (const node of nodes) {
		const direct =
			node.type === 'heading' || node.type === 'text' || node.type === 'badge'
				? node.text
				: node.type === 'stat'
					? node.label
					: null;
		if (direct) return direct.slice(0, MAX_LABEL_LENGTH);
		const childNodes: UiNode[] =
			node.type === 'stack' || node.type === 'card' || node.type === 'grid'
				? node.children
				: node.type === 'list'
					? node.items
					: node.type === 'keyValue'
						? node.items.map((item) => item.value)
						: node.type === 'table'
							? node.rows.flatMap((row) => Object.values(row.cells))
							: [];
		const found = findLabel(childNodes);
		if (found) return found;
	}
	return null;
};

/**
 * Plugin UI is data, never markup: only these components, only plain strings and numbers, bounded size.
 * Whatever does not fit is dropped rather than rendered.
 */
export const sanitizeTree = (
	raw: unknown,
	context: LinkContext = NO_LINKS,
	apiMinor: number = CURRENT_API_MINOR,
): UiNode | null => {
	let budget = MAX_NODES;
	let longBudget = MAX_LONG_TEXT_TOTAL;
	const longText = (value: unknown): string | null => {
		if (typeof value !== 'string') return null;
		const result = value.slice(
			0,
			longBudget > 0 && apiMinor >= 7 ? MAX_LONG_TEXT : MAX_TEXT,
		);
		longBudget -= result.length;
		return result;
	};
	const node = (
		value: any,
		depth: number,
		cardDepth: number,
	): UiNode | null => {
		if (depth > MAX_DEPTH || budget-- <= 0) return null;
		const plain = text(value);
		if (plain !== null) return { type: 'text', text: plain, tone: 'default' };
		if (!value || typeof value !== 'object') return null;
		const children = (items: unknown, childCardDepth: number) =>
			(Array.isArray(items) ? items.slice(0, MAX_ITEMS) : [])
				.map((child) => node(child, depth + 1, childCardDepth))
				.filter((child): child is UiNode => child !== null);
		switch (value.type) {
			case 'stack': {
				const base = {
					type: 'stack' as const,
					direction:
						value.direction === 'row' ? ('row' as const) : ('column' as const),
					children: children(value.children, cardDepth),
				};
				if (apiMinor < 3) return base;
				const gap = pickOptional(value.gap, GAPS);
				const align = pickOptional(value.align, ALIGNS);
				const justify = pickOptional(value.justify, JUSTIFIES);
				const grow = value.grow === true ? true : undefined;
				return {
					...base,
					...(gap !== undefined ? { gap } : {}),
					...(align !== undefined ? { align } : {}),
					...(justify !== undefined ? { justify } : {}),
					...(grow !== undefined ? { grow } : {}),
				};
			}
			case 'card': {
				if (apiMinor < 3 || cardDepth >= MAX_CARD_DEPTH) {
					return {
						type: 'stack',
						direction: 'column',
						children: children(value.children, cardDepth),
					};
				}
				const kids = children(value.children, cardDepth + 1);
				const tone = pick(value.tone, CARD_TONES, 'default');
				const padding = pick(value.padding, PADDINGS, 'md');
				const accent =
					value.accent === undefined
						? undefined
						: pick(value.accent, COLORS, 'gray');
				const grow = value.grow === true ? true : undefined;
				const onClick = parseCommandArgs(value.onClick);
				const label = onClick ? (findLabel(kids) ?? 'Card') : undefined;
				return {
					type: 'card',
					children: kids,
					tone,
					padding,
					...(accent !== undefined ? { accent } : {}),
					...(grow !== undefined ? { grow } : {}),
					...(onClick ? { onClick } : {}),
					...(label !== undefined ? { label } : {}),
				};
			}
			case 'grid': {
				if (apiMinor < 3) {
					return {
						type: 'stack',
						direction: 'row',
						children: children(value.children, cardDepth),
					};
				}
				const rawColumns =
					typeof value.columns === 'number' && Number.isFinite(value.columns)
						? Math.round(value.columns)
						: 1;
				const columns = Math.max(1, Math.min(6, rawColumns));
				const rawMinWidth =
					typeof value.minWidth === 'number' && Number.isFinite(value.minWidth)
						? value.minWidth
						: 200;
				const minWidth = Math.max(160, Math.min(480, rawMinWidth));
				const gap = pick(value.gap, GRID_GAPS, 'md');
				return {
					type: 'grid',
					columns,
					minWidth,
					gap,
					children: children(value.children, cardDepth),
				};
			}
			case 'menu': {
				if (apiMinor < 3)
					return { type: 'stack', direction: 'column', children: [] };
				const items = (
					Array.isArray(value.items) ? value.items.slice(0, MAX_MENU_ITEMS) : []
				)
					// Items count against the node budget, like table rows, so nesting cannot multiply it.
					.filter(() => budget-- > 0)
					.map((item: any) => {
						const t = text(item?.text);
						const cmd = parseCommandArgs(item);
						if (t === null || cmd === null) return null;
						const confirmText =
							typeof item?.confirm === 'string' ? item.confirm.trim() : '';
						const confirm = confirmText ? confirmText.slice(0, 200) : undefined;
						return {
							text: t,
							...cmd,
							...(confirm !== undefined ? { confirm } : {}),
						};
					})
					.filter((item: any): item is MenuItem => item !== null);
				if (items.length === 0) return null;
				const label = text(value.label)?.slice(0, 60);
				return {
					type: 'menu',
					icon: 'more',
					items,
					...(label !== undefined && label !== null ? { label } : {}),
				};
			}
			case 'heading': {
				const t = text(value.text);
				return t === null
					? null
					: {
							type: 'heading',
							text: t,
							level: pick(value.level, [1, 2, 3] as const, 2),
						};
			}
			case 'text': {
				const tone = pick(value.tone, TONES, 'default');
				if (apiMinor < 7) {
					const t = text(value.text);
					return t === null ? null : { type: 'text', text: t, tone };
				}
				const spans: TextSpan[] = [];
				let joined = '';
				if (Array.isArray(value.spans)) {
					for (const span of value.spans.slice(0, MAX_SPANS)) {
						if (typeof span?.text !== 'string' || span.text === '') continue;
						const room =
							(longBudget > 0 ? MAX_LONG_TEXT : MAX_TEXT) - joined.length;
						if (room <= 0) break;
						const piece = span.text.slice(0, room);
						joined += piece;
						spans.push({
							text: piece,
							...Object.fromEntries(
								SPAN_FLAGS.filter(([key]) => span[key] === true).map(
									([key]) => [key, true],
								),
							),
						});
					}
				}
				let t: string | null;
				if (spans.length > 0) {
					longBudget -= joined.length;
					t = joined;
				} else {
					t = longText(value.text) ?? text(value.text);
					if (t === null) return null;
				}
				return {
					type: 'text',
					text: t,
					tone,
					...(spans.length > 0 ? { spans } : {}),
					...(t.includes('\n') ? { breaks: true as const } : {}),
				};
			}
			case 'markdown': {
				if (apiMinor < 7) return null;
				const source = longText(value.text);
				if (source === null) return null;
				let tokens: Token[];
				try {
					tokens = markdownLexer.lexer(source);
				} catch {
					return { type: 'text', text: source, tone: 'default', breaks: true };
				}
				const blocks = convertBlocks(
					tokens,
					{ take: () => budget-- > 0, links: context },
					0,
				);
				return { type: 'markdown', blocks };
			}
			case 'code': {
				if (apiMinor < 7) return null;
				const t = longText(value.text);
				if (t === null) return null;
				const lang =
					typeof value.lang === 'string'
						? value.lang.trim().slice(0, MAX_LANG)
						: '';
				return {
					type: 'code',
					text: t,
					...(lang ? { lang } : {}),
					wrap: value.wrap !== false,
				};
			}
			case 'badge': {
				const t = text(value.text);
				if (t === null) return null;
				const cmd = apiMinor >= 3 ? parseCommandArgs(value) : null;
				return {
					type: 'badge',
					text: t.slice(0, 40),
					color: pick(value.color, COLORS, 'gray'),
					...(cmd ? cmd : {}),
				};
			}
			case 'stat': {
				const label = text(value.label);
				const v = text(value.value);
				if (label === null || v === null) return null;
				const cmd = apiMinor >= 3 ? parseCommandArgs(value) : null;
				return {
					type: 'stat',
					label,
					value: v,
					tone: pick(value.tone, TONES, 'default'),
					...(cmd ? cmd : {}),
				};
			}
			case 'progress':
				return typeof value.value === 'number' && Number.isFinite(value.value)
					? {
							type: 'progress',
							value: Math.max(0, Math.min(10, value.value)),
							color: pick(value.color, COLORS, 'blue'),
						}
					: null;
			case 'list':
				return { type: 'list', items: children(value.items, cardDepth) };
			case 'table': {
				const columns = (
					Array.isArray(value.columns) ? value.columns.slice(0, 12) : []
				)
					.map((c: any) => ({ key: text(c?.key), title: text(c?.title) }))
					.filter(
						(c: any): c is { key: string; title: string } =>
							c.key !== null && c.title !== null,
					);
				const rows = (
					Array.isArray(value.rows) ? value.rows.slice(0, MAX_ITEMS) : []
				)
					// Rows count against the same budget as nodes, so nested tables cannot multiply.
					.filter(() => budget-- > 0)
					.map((row: any) => {
						const cells: Record<string, UiNode> = {};
						for (const { key } of columns) {
							const cell = node(row?.cells?.[key], depth + 1, cardDepth);
							if (cell) cells[key] = cell;
						}
						return Number.isSafeInteger(row?.taskId) && row.taskId > 0
							? { taskId: row.taskId, cells }
							: { cells };
					});
				return { type: 'table', columns, rows };
			}
			case 'button': {
				const t = text(value.text);
				const cmd = parseCommandArgs(value);
				if (t === null || cmd === null) return null;
				const confirmText =
					typeof value.confirm === 'string' ? value.confirm.trim() : '';
				const confirm = confirmText ? confirmText.slice(0, 200) : undefined;
				const variant =
					apiMinor >= 3
						? pickOptional(value.variant, BUTTON_VARIANTS)
						: undefined;
				const size =
					apiMinor >= 3 ? pickOptional(value.size, BUTTON_SIZES) : undefined;
				return {
					type: 'button',
					text: t,
					...cmd,
					...(confirm !== undefined ? { confirm } : {}),
					...(variant !== undefined ? { variant } : {}),
					...(size !== undefined ? { size } : {}),
				};
			}
			case 'taskLink': {
				const t = text(value.text);
				return Number.isSafeInteger(value.taskId) &&
					value.taskId > 0 &&
					t !== null
					? { type: 'taskLink', taskId: value.taskId, text: t }
					: null;
			}
			case 'divider':
				return { type: 'divider' };
			case 'copyable': {
				const t =
					typeof value.text === 'string'
						? value.text.slice(0, MAX_COPYABLE_TEXT)
						: null;
				if (t === null) return null;
				const label = text(value.label);
				return label === null
					? { type: 'copyable', text: t }
					: { type: 'copyable', text: t, label };
			}
			case 'link': {
				const url =
					typeof value.url === 'string' ? value.url.slice(0, 2000) : null;
				if (url === null) return null;
				const label = text(value.text) ?? url;
				if (!isLinkAllowed(url, context)) {
					return { type: 'text', text: label, tone: 'default' };
				}
				let host: string;
				try {
					host = new URL(url).hostname;
				} catch {
					return null;
				}
				return { type: 'link', url, text: label, host };
			}
			case 'timeAgo':
			case 'dueTime': {
				const at =
					typeof value.at === 'string' && value.at.length <= MAX_AT_LENGTH
						? value.at
						: null;
				return at === null || Number.isNaN(Date.parse(at))
					? null
					: { type: value.type, at };
			}
			case 'keyValue': {
				const items = (
					Array.isArray(value.items)
						? value.items.slice(0, MAX_KEY_VALUE_ITEMS)
						: []
				)
					// Rows count against the node budget, like table rows, so nesting cannot multiply it.
					.filter(() => budget-- > 0)
					.map((item: any) => {
						const key = text(item?.key)?.slice(0, 60) ?? null;
						const value_ = node(item?.value, depth + 1, cardDepth);
						return key !== null && value_ !== null
							? { key, value: value_ }
							: null;
					})
					.filter(
						(item: any): item is { key: string; value: UiNode } =>
							item !== null,
					);
				return { type: 'keyValue', items };
			}
			default:
				return null;
		}
	};
	return node(raw, 0, 0);
};
