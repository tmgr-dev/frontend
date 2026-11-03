import { Lexer, type Token } from 'marked';
import { PLUGIN_API_VERSION } from './manifest';

export type Tone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
export type Color =
	'gray' | 'green' | 'yellow' | 'red' | 'blue' | 'purple' | 'orange';

export type ColumnAlign = 'left' | 'center' | 'right';
export type ColumnWidth = 'xs' | 'sm' | 'md' | 'lg';
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

export const ICON_NAMES = [
	'check',
	'x',
	'plus',
	'minus',
	'alert-triangle',
	'info',
	'circle-help',
	'clock',
	'calendar',
	'user',
	'users',
	'star',
	'heart',
	'flag',
	'tag',
	'link',
	'external-link',
	'copy',
	'trash',
	'edit',
	'pencil',
	'search',
	'filter',
	'settings',
	'refresh',
	'play',
	'pause',
	'download',
	'upload',
	'git-branch',
	'git-pull-request',
	'bug',
	'zap',
	'lock',
] as const;
export type IconName = (typeof ICON_NAMES)[number];

export type ButtonVariant =
	'default' | 'primary' | 'ghost' | 'danger' | 'success' | 'outline';

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
			icon?: IconName;
			tooltip?: string;
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
			columns: {
				key: string;
				title: string;
				align?: ColumnAlign;
				width?: ColumnWidth;
				sortable?: true;
			}[];
			rows: {
				taskId?: number;
				cells: Record<string, UiNode>;
				sort?: Record<string, string | number>;
				color?: Color;
				onClick?: Command;
				expand?: UiNode[];
			}[];
	  }
	| {
			type: 'timeline';
			items: { at: string; title?: string; text: string; tone: Color }[];
	  }
	| {
			type: 'messages';
			items: {
				author: string;
				text: string;
				at?: string;
				side: 'left' | 'right';
			}[];
	  }
	| { type: 'sparkline'; values: number[]; color: Color; label?: string }
	| { type: 'empty'; title: string; text?: string; icon?: IconName }
	| {
			type: 'skeleton';
			variant: 'text' | 'card' | 'table';
			lines: number;
	  }
	| {
			type: 'button';
			text: string;
			command: string;
			args?: unknown;
			confirm?: string;
			variant?: ButtonVariant;
			size?: 'sm' | 'md';
			icon?: IconName;
			iconOnly?: true;
			disabled?: true;
			loading?: true;
			tooltip?: string;
	  }
	| { type: 'buttonGroup'; children: UiNode[] }
	| { type: 'icon'; name: IconName; tone: Tone }
	| {
			type: 'collapse';
			title?: string;
			open: boolean;
			visible?: number;
			children: UiNode[];
	  }
	| {
			type: 'tabs';
			tabs: { id: string; title: string; badge?: string; children: UiNode[] }[];
			selected: string;
	  }
	| {
			type: 'segmented';
			options: { value: string; text: string }[];
			value: string | null;
			command: string;
			args?: unknown;
	  }
	| {
			type: 'input';
			name: string;
			placeholder?: string;
			value: string;
			command: string;
			args?: unknown;
			mode: 'change' | 'submit';
			debounce: number;
	  }
	| {
			type: 'select';
			name: string;
			options: { value: string; text: string }[];
			value: string | null;
			placeholder?: string;
			command: string;
			args?: unknown;
	  }
	| {
			type: 'checkbox';
			name: string;
			label: string;
			checked: boolean;
			command: string;
			args?: unknown;
	  }
	| {
			type: 'textarea';
			name: string;
			value: string;
			placeholder?: string;
			rows: number;
			submitText?: string;
			command: string;
			args?: unknown;
	  }
	| {
			type: 'date';
			name: string;
			value: string | null;
			min?: string;
			max?: string;
			command: string;
			args?: unknown;
	  }
	| { type: 'taskLink'; taskId: number; text: string }
	| { type: 'divider' }
	| { type: 'copyable'; text: string; label?: string }
	| { type: 'link'; url: string; text: string; host: string }
	| { type: 'timeAgo'; at: string }
	| { type: 'dueTime'; at: string }
	| { type: 'keyValue'; items: { key: string; value: UiNode }[] };

type UiTable = Extract<UiNode, { type: 'table' }>;
type UiColumn = UiTable['columns'][number];
type UiRow = UiTable['rows'][number];
type UiTimelineItem = Extract<UiNode, { type: 'timeline' }>['items'][number];
type UiMessage = Extract<UiNode, { type: 'messages' }>['items'][number];

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
const MAX_MD_INLINE = 4000;
const MD_TIME_BUDGET_MS = 250;
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
const MAX_TOOLTIP = 200;
const MAX_TITLE = 200;
const MAX_TABS = 12;
const MAX_TAB_ID = 64;
const MAX_BADGE_TEXT = 40;
const MAX_BUTTON_GROUP = 12;
const MAX_SEGMENTED_VALUE = 64;
const MIN_SEGMENTED = 2;
export const MAX_INPUT_VALUE = 500;
export const MAX_TEXTAREA_VALUE = 5000;
const MIN_TEXTAREA_ROWS = 2;
const MAX_TEXTAREA_ROWS = 12;
const DEFAULT_TEXTAREA_ROWS = 4;
const MAX_SUBMIT_TEXT = 40;
const MIN_DATE_YEAR = 1000;
const MAX_INPUT_PLACEHOLDER = 200;
const MAX_CONTROL_NAME = 64;
const MAX_SELECT_OPTIONS = 100;
const MAX_SELECT_VALUE = 200;
export const DEBOUNCE_MIN = 150;
export const DEBOUNCE_MAX = 2000;
export const DEBOUNCE_DEFAULT = 300;
const CONTROL_NAME = /^[A-Za-z0-9_.-]+$/;
const MAX_SEGMENTED = 4;
const MAX_FEED_ITEMS = 200;
const MAX_AUTHOR = 64;
const MAX_SPARKLINE_VALUES = 365;
const MAX_ROW_EXPAND = 50;
const MAX_SORT_TEXT = 200;
const MAX_SKELETON_LINES = 12;
const DEFAULT_SKELETON_LINES = 3;
const COLUMN_ALIGNS: ColumnAlign[] = ['left', 'center', 'right'];
const COLUMN_WIDTHS: ColumnWidth[] = ['xs', 'sm', 'md', 'lg'];
const SKELETON_VARIANTS = ['text', 'card', 'table'] as const;
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
const BUTTON_VARIANTS: ButtonVariant[] = ['default', 'primary', 'ghost'];
const BUTTON_VARIANTS_V8: ButtonVariant[] = [
	...BUTTON_VARIANTS,
	'danger',
	'success',
	'outline',
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

const lexMarkdown = (source: string, deadline: number): Token[] => {
	const lexer = new Lexer({ gfm: true, breaks: true });
	lexer.blockTokens(source.replace(/\r\n?/g, '\n'), lexer.tokens);
	for (const { src, tokens } of lexer.inlineQueue) {
		if (src.length > MAX_MD_INLINE || Date.now() > deadline) {
			tokens.push({ type: 'text', raw: src, text: src });
		} else {
			lexer.inlineTokens(src, tokens);
		}
	}
	lexer.inlineQueue = [];
	return lexer.tokens;
};

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

const shortText = (value: unknown, max: number): string | undefined => {
	if (typeof value !== 'string') return undefined;
	const trimmed = value.trim().slice(0, max);
	return trimmed === '' ? undefined : trimmed;
};

export const clampDebounce = (value: unknown): number =>
	typeof value === 'number' && Number.isFinite(value)
		? Math.min(DEBOUNCE_MAX, Math.max(DEBOUNCE_MIN, Math.round(value)))
		: DEBOUNCE_DEFAULT;

export const cutCodePoints = (value: string, max: number): string => {
	let end = 0;
	for (let count = 0; end < value.length && count < max; count++) {
		const unit = value.charCodeAt(end);
		const pair =
			unit >= 0xd800 &&
			unit <= 0xdbff &&
			(value.charCodeAt(end + 1) & 0xfc00) === 0xdc00;
		end += pair ? 2 : 1;
	}
	return value.slice(0, end);
};

export const calendarDate = (value: unknown): string | null => {
	if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value))
		return null;
	const year = Number(value.slice(0, 4));
	const month = Number(value.slice(5, 7));
	const day = Number(value.slice(8, 10));
	if (year < MIN_DATE_YEAR || month < 1 || month > 12 || day < 1) return null;
	return day <= new Date(Date.UTC(year, month, 0)).getUTCDate() ? value : null;
};

const controlName = (value: unknown): string | null =>
	typeof value === 'string' &&
	value.length <= MAX_CONTROL_NAME &&
	CONTROL_NAME.test(value)
		? value
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
	let mdDeadline: number | null = null;
	const longRoom = () =>
		apiMinor >= 7
			? MAX_TEXT + Math.min(MAX_LONG_TEXT - MAX_TEXT, longBudget)
			: MAX_TEXT;
	const chargeLong = (length: number) => {
		longBudget -= Math.max(0, length - MAX_TEXT);
	};
	const longText = (value: unknown): string | null => {
		if (typeof value !== 'string') return null;
		const result = value.slice(0, longRoom());
		chargeLong(result.length);
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
						const room = longRoom() - joined.length;
						if (room <= 0 || budget-- <= 0) break;
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
					chargeLong(joined.length);
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
				mdDeadline ??= Date.now() + MD_TIME_BUDGET_MS;
				if (Date.now() > mdDeadline) {
					return { type: 'text', text: source, tone: 'default', breaks: true };
				}
				let tokens: Token[];
				try {
					tokens = lexMarkdown(source, mdDeadline);
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
				const icon =
					apiMinor >= 8 ? pickOptional(value.icon, ICON_NAMES) : undefined;
				const tooltip =
					apiMinor >= 8 ? shortText(value.tooltip, MAX_TOOLTIP) : undefined;
				return {
					type: 'badge',
					text: t.slice(0, MAX_BADGE_TEXT),
					color: pick(value.color, COLORS, 'gray'),
					...(cmd ? cmd : {}),
					...(icon !== undefined ? { icon } : {}),
					...(tooltip !== undefined ? { tooltip } : {}),
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
					.map((c: any) => {
						const key = text(c?.key);
						const title = text(c?.title);
						if (key === null || title === null) return null;
						const align =
							apiMinor >= 10
								? pickOptional(c?.align, COLUMN_ALIGNS)
								: undefined;
						const width =
							apiMinor >= 10
								? pickOptional(c?.width, COLUMN_WIDTHS)
								: undefined;
						const sortable =
							apiMinor >= 10 && c?.sortable === true
								? (true as const)
								: undefined;
						return {
							key,
							title,
							...(align !== undefined ? { align } : {}),
							...(width !== undefined ? { width } : {}),
							...(sortable !== undefined ? { sortable } : {}),
						};
					})
					.filter((c: any): c is UiColumn => c !== null);
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
						const out: UiRow = { cells };
						if (Number.isSafeInteger(row?.taskId) && row.taskId > 0) {
							out.taskId = row.taskId;
						}
						if (apiMinor >= 10) {
							const color = pickOptional(row?.color, COLORS);
							if (color !== undefined) out.color = color;
							const onClick =
								row?.onClick !== null && typeof row?.onClick === 'object'
									? parseCommandArgs(row.onClick)
									: null;
							if (onClick) out.onClick = onClick;
							const sort: Record<string, string | number> = {};
							for (const { key } of columns) {
								const v = row?.sort?.[key];
								if (typeof v === 'number' && Number.isFinite(v)) sort[key] = v;
								else if (typeof v === 'string')
									sort[key] = v.slice(0, MAX_SORT_TEXT);
							}
							if (Object.keys(sort).length > 0) out.sort = sort;
							const expand = (
								Array.isArray(row?.expand)
									? row.expand.slice(0, MAX_ROW_EXPAND)
									: []
							)
								.map((child: any) => node(child, depth + 1, cardDepth))
								.filter(
									(child: UiNode | null): child is UiNode => child !== null,
								);
							if (expand.length > 0) out.expand = expand;
						}
						return out;
					});
				return { type: 'table', columns, rows };
			}
			case 'timeline': {
				if (apiMinor < 10) return null;
				const items = (
					Array.isArray(value.items) ? value.items.slice(0, MAX_FEED_ITEMS) : []
				)
					.filter(() => budget-- > 0)
					.map((item: any) => {
						const at = shortText(item?.at, MAX_AT_LENGTH);
						const t = text(item?.text);
						if (at === undefined || t === null) return null;
						const title = shortText(item?.title, MAX_TITLE);
						return {
							at,
							text: t,
							tone: pick(item?.tone, COLORS, 'gray'),
							...(title !== undefined ? { title } : {}),
						};
					})
					.filter((item: any): item is UiTimelineItem => item !== null);
				return { type: 'timeline', items };
			}
			case 'messages': {
				if (apiMinor < 10) return null;
				const items = (
					Array.isArray(value.items) ? value.items.slice(0, MAX_FEED_ITEMS) : []
				)
					.filter(() => budget-- > 0)
					.map((item: any) => {
						const author = shortText(item?.author, MAX_AUTHOR);
						const t = text(item?.text);
						if (author === undefined || t === null) return null;
						const at = shortText(item?.at, MAX_AT_LENGTH);
						return {
							author,
							text: t,
							side:
								item?.side === 'right' ? ('right' as const) : ('left' as const),
							...(at !== undefined ? { at } : {}),
						};
					})
					.filter((item: any): item is UiMessage => item !== null);
				return { type: 'messages', items };
			}
			case 'sparkline': {
				if (apiMinor < 10) return null;
				const values: number[] = [];
				for (const v of Array.isArray(value.values) ? value.values : []) {
					if (values.length >= MAX_SPARKLINE_VALUES) break;
					if (typeof v !== 'number' || !Number.isFinite(v)) continue;
					if (budget-- <= 0) break;
					values.push(v);
				}
				const label = shortText(value.label, MAX_LABEL_LENGTH);
				return {
					type: 'sparkline',
					values,
					color: pick(value.color, COLORS, 'blue'),
					...(label !== undefined ? { label } : {}),
				};
			}
			case 'empty': {
				if (apiMinor < 10) return null;
				const title = shortText(value.title, MAX_TITLE);
				if (title === undefined) return null;
				const body = shortText(value.text, MAX_TEXT);
				const icon = pickOptional(value.icon, ICON_NAMES);
				return {
					type: 'empty',
					title,
					...(body !== undefined ? { text: body } : {}),
					...(icon !== undefined ? { icon } : {}),
				};
			}
			case 'skeleton': {
				if (apiMinor < 10) return null;
				const lines =
					typeof value.lines === 'number' && Number.isFinite(value.lines)
						? Math.min(MAX_SKELETON_LINES, Math.max(1, Math.round(value.lines)))
						: DEFAULT_SKELETON_LINES;
				return {
					type: 'skeleton',
					variant: pick(value.variant, SKELETON_VARIANTS, 'text'),
					lines,
				};
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
						? pickOptional(
								value.variant,
								apiMinor >= 8 ? BUTTON_VARIANTS_V8 : BUTTON_VARIANTS,
							)
						: undefined;
				const size =
					apiMinor >= 3 ? pickOptional(value.size, BUTTON_SIZES) : undefined;
				const icon =
					apiMinor >= 8 ? pickOptional(value.icon, ICON_NAMES) : undefined;
				const tooltip =
					apiMinor >= 8 ? shortText(value.tooltip, MAX_TOOLTIP) : undefined;
				const flag = (key: string) =>
					apiMinor >= 8 && value[key] === true ? (true as const) : undefined;
				const iconOnly = icon !== undefined ? flag('iconOnly') : undefined;
				const disabled = flag('disabled');
				const loading = flag('loading');
				return {
					type: 'button',
					text: t,
					...cmd,
					...(confirm !== undefined ? { confirm } : {}),
					...(variant !== undefined ? { variant } : {}),
					...(size !== undefined ? { size } : {}),
					...(icon !== undefined ? { icon } : {}),
					...(iconOnly !== undefined ? { iconOnly } : {}),
					...(disabled !== undefined ? { disabled } : {}),
					...(loading !== undefined ? { loading } : {}),
					...(tooltip !== undefined ? { tooltip } : {}),
				};
			}
			case 'buttonGroup': {
				if (apiMinor < 8) return null;
				const kids = (Array.isArray(value.children) ? value.children : [])
					.filter((child: any) => child?.type === 'button')
					.slice(0, MAX_BUTTON_GROUP)
					.map((child: any) => node(child, depth + 1, cardDepth))
					.filter((child: UiNode | null): child is UiNode => child !== null);
				return kids.length === 0
					? null
					: { type: 'buttonGroup', children: kids };
			}
			case 'icon': {
				if (apiMinor < 8) return null;
				const name = pickOptional(value.name, ICON_NAMES);
				return name === undefined
					? null
					: { type: 'icon', name, tone: pick(value.tone, TONES, 'default') };
			}
			case 'collapse': {
				if (apiMinor < 8) return null;
				const title = shortText(value.title, MAX_TITLE);
				const visible =
					Number.isSafeInteger(value.visible) && value.visible >= 1
						? Math.min(value.visible, MAX_ITEMS)
						: undefined;
				return {
					type: 'collapse',
					open: value.open === true,
					children: children(value.children, cardDepth),
					...(title !== undefined ? { title } : {}),
					...(visible !== undefined ? { visible } : {}),
				};
			}
			case 'tabs': {
				if (apiMinor < 8) return null;
				const seen = new Set<string>();
				const tabs = (Array.isArray(value.tabs) ? value.tabs : [])
					.slice(0, MAX_TABS)
					.filter(() => budget-- > 0)
					.map((tab: any) => {
						const id =
							typeof tab?.id === 'string' &&
							tab.id !== '' &&
							tab.id.length <= MAX_TAB_ID
								? tab.id
								: null;
						const title = shortText(tab?.title, MAX_TITLE);
						if (id === null || title === undefined || seen.has(id)) return null;
						seen.add(id);
						const badge = shortText(tab.badge, MAX_BADGE_TEXT);
						return {
							id,
							title,
							...(badge !== undefined ? { badge } : {}),
							children: children(tab.children, cardDepth),
						};
					})
					.filter((tab: any): tab is NonNullable<typeof tab> => tab !== null);
				if (tabs.length === 0) return null;
				const selected = tabs.some((tab: any) => tab.id === value.selected)
					? (value.selected as string)
					: tabs[0].id;
				return { type: 'tabs', tabs, selected };
			}
			case 'segmented': {
				if (apiMinor < 8) return null;
				const rawOptions = Array.isArray(value.options) ? value.options : [];
				if (
					rawOptions.length < MIN_SEGMENTED ||
					rawOptions.length > MAX_SEGMENTED
				)
					return null;
				const seen = new Set<string>();
				const options = rawOptions
					.filter(() => budget-- > 0)
					.map((option: any) => {
						const v =
							typeof option?.value === 'string' &&
							option.value !== '' &&
							option.value.length <= MAX_SEGMENTED_VALUE
								? option.value
								: null;
						const t = text(option?.text);
						if (v === null || t === null || seen.has(v)) return null;
						seen.add(v);
						return { value: v, text: t };
					})
					.filter(
						(option: any): option is { value: string; text: string } =>
							option !== null,
					);
				const cmd = parseCommandArgs(value);
				if (options.length < MIN_SEGMENTED || cmd === null) return null;
				return {
					type: 'segmented',
					options,
					value: seen.has(value.value) ? value.value : null,
					...cmd,
				};
			}
			case 'input': {
				if (apiMinor < 9) return null;
				const name = controlName(value.name);
				const cmd = parseCommandArgs(value);
				if (name === null || cmd === null) return null;
				const placeholder =
					typeof value.placeholder === 'string' && value.placeholder !== ''
						? value.placeholder.slice(0, MAX_INPUT_PLACEHOLDER)
						: undefined;
				return {
					type: 'input',
					name,
					value:
						typeof value.value === 'string'
							? value.value.slice(0, MAX_INPUT_VALUE)
							: '',
					mode: value.mode === 'submit' ? 'submit' : 'change',
					debounce: clampDebounce(value.debounce),
					...(placeholder !== undefined ? { placeholder } : {}),
					...cmd,
				};
			}
			case 'select': {
				if (apiMinor < 9) return null;
				const name = controlName(value.name);
				const cmd = parseCommandArgs(value);
				if (name === null || cmd === null) return null;
				const seen = new Set<string>();
				const options = (Array.isArray(value.options) ? value.options : [])
					.slice(0, MAX_SELECT_OPTIONS)
					.filter(() => budget-- > 0)
					.map((option: any) => {
						const v =
							typeof option?.value === 'string' &&
							option.value !== '' &&
							option.value.length <= MAX_SELECT_VALUE
								? option.value
								: null;
						const t = text(option?.text);
						if (v === null || t === null || seen.has(v)) return null;
						seen.add(v);
						return { value: v, text: t };
					})
					.filter(
						(option: any): option is { value: string; text: string } =>
							option !== null,
					);
				if (options.length === 0) return null;
				const placeholder =
					typeof value.placeholder === 'string' && value.placeholder !== ''
						? value.placeholder.slice(0, MAX_INPUT_PLACEHOLDER)
						: undefined;
				return {
					type: 'select',
					name,
					options,
					value: seen.has(value.value) ? value.value : null,
					...(placeholder !== undefined ? { placeholder } : {}),
					...cmd,
				};
			}
			case 'checkbox': {
				if (apiMinor < 9) return null;
				const name = controlName(value.name);
				const label = text(value.label);
				const cmd = parseCommandArgs(value);
				if (name === null || label === null || cmd === null) return null;
				return {
					type: 'checkbox',
					name,
					label,
					checked: value.checked === true,
					...cmd,
				};
			}
			case 'textarea': {
				if (apiMinor < 11) return null;
				const name = controlName(value.name);
				const cmd = parseCommandArgs(value);
				if (name === null || cmd === null) return null;
				const placeholder =
					typeof value.placeholder === 'string' && value.placeholder !== ''
						? cutCodePoints(value.placeholder, MAX_INPUT_PLACEHOLDER)
						: undefined;
				const submitText = shortText(value.submitText, MAX_SUBMIT_TEXT);
				return {
					type: 'textarea',
					name,
					value:
						typeof value.value === 'string'
							? cutCodePoints(value.value, MAX_TEXTAREA_VALUE)
							: '',
					rows:
						typeof value.rows === 'number' && Number.isFinite(value.rows)
							? Math.min(
									MAX_TEXTAREA_ROWS,
									Math.max(MIN_TEXTAREA_ROWS, Math.round(value.rows)),
								)
							: DEFAULT_TEXTAREA_ROWS,
					...(placeholder !== undefined ? { placeholder } : {}),
					...(submitText !== undefined ? { submitText } : {}),
					...cmd,
				};
			}
			case 'date': {
				if (apiMinor < 11) return null;
				const name = controlName(value.name);
				const cmd = parseCommandArgs(value);
				if (name === null || cmd === null) return null;
				let min = calendarDate(value.min);
				let max = calendarDate(value.max);
				if (min !== null && max !== null && min > max) {
					min = null;
					max = null;
				}
				const picked = calendarDate(value.value);
				return {
					type: 'date',
					name,
					value:
						picked !== null &&
						(min === null || picked >= min) &&
						(max === null || picked <= max)
							? picked
							: null,
					...(min !== null ? { min } : {}),
					...(max !== null ? { max } : {}),
					...cmd,
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
