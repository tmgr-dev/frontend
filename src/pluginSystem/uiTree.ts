import { PLUGIN_API_VERSION } from './manifest';

export type Tone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
export type Color =
	| 'gray'
	| 'green'
	| 'yellow'
	| 'red'
	| 'blue'
	| 'purple'
	| 'orange';

export type Gap = 'none' | 'sm' | 'md' | 'lg';
export type Command = { command: string; args?: unknown };
export type MenuItem = { text: string; command: string; args?: unknown; confirm?: string };

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
	| { type: 'text'; text: string; tone: Tone }
	| { type: 'badge'; text: string; color: Color; command?: string; args?: unknown }
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
const CARD_TONES: ('default' | 'muted' | 'raised')[] = ['default', 'muted', 'raised'];
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
		parsed.protocol === 'https:' && context.allowedDomains.includes(parsed.hostname)
	);
};

const pick = <T>(value: unknown, allowed: readonly T[], fallback: T): T =>
	allowed.includes(value as T) ? (value as T) : fallback;

const pickOptional = <T>(value: unknown, allowed: readonly T[]): T | undefined =>
	allowed.includes(value as T) ? (value as T) : undefined;

const text = (value: unknown): string | null =>
	typeof value === 'string'
		? value.slice(0, MAX_TEXT)
		: typeof value === 'number' && Number.isFinite(value)
		? String(value)
		: null;

const parseCommandArgs = (value: any): { command: string; args?: unknown } | null => {
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
	const node = (value: any, depth: number, cardDepth: number): UiNode | null => {
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
					direction: value.direction === 'row' ? ('row' as const) : ('column' as const),
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
					value.accent === undefined ? undefined : pick(value.accent, COLORS, 'gray');
				const grow = value.grow === true ? true : undefined;
				const onClick = parseCommandArgs(value.onClick);
				const label = onClick ? findLabel(kids) ?? 'Card' : undefined;
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
				if (apiMinor < 3) return { type: 'stack', direction: 'column', children: [] };
				const items = (Array.isArray(value.items) ? value.items.slice(0, MAX_MENU_ITEMS) : [])
					// Items count against the node budget, like table rows, so nesting cannot multiply it.
					.filter(() => budget-- > 0)
					.map((item: any) => {
						const t = text(item?.text);
						const cmd = parseCommandArgs(item);
						if (t === null || cmd === null) return null;
						const confirmText =
							typeof item?.confirm === 'string' ? item.confirm.trim() : '';
						const confirm = confirmText ? confirmText.slice(0, 200) : undefined;
						return { text: t, ...cmd, ...(confirm !== undefined ? { confirm } : {}) };
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
				const t = text(value.text);
				return t === null
					? null
					: { type: 'text', text: t, tone: pick(value.tone, TONES, 'default') };
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
				const variant = apiMinor >= 3 ? pickOptional(value.variant, BUTTON_VARIANTS) : undefined;
				const size = apiMinor >= 3 ? pickOptional(value.size, BUTTON_SIZES) : undefined;
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
				const url = typeof value.url === 'string' ? value.url.slice(0, 2000) : null;
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
					Array.isArray(value.items) ? value.items.slice(0, MAX_KEY_VALUE_ITEMS) : []
				)
					// Rows count against the node budget, like table rows, so nesting cannot multiply it.
					.filter(() => budget-- > 0)
					.map((item: any) => {
						const key = text(item?.key)?.slice(0, 60) ?? null;
						const value_ = node(item?.value, depth + 1, cardDepth);
						return key !== null && value_ !== null ? { key, value: value_ } : null;
					})
					.filter(
						(item: any): item is { key: string; value: UiNode } => item !== null,
					);
				return { type: 'keyValue', items };
			}
			default:
				return null;
		}
	};
	return node(raw, 0, 0);
};
