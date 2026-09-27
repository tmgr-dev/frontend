export type Tone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
export type Color =
	| 'gray'
	| 'green'
	| 'yellow'
	| 'red'
	| 'blue'
	| 'purple'
	| 'orange';

export type UiNode =
	| { type: 'stack'; direction: 'row' | 'column'; children: UiNode[] }
	| { type: 'heading'; text: string; level: 1 | 2 | 3 }
	| { type: 'text'; text: string; tone: Tone }
	| { type: 'badge'; text: string; color: Color }
	| { type: 'stat'; label: string; value: string; tone: Tone }
	| { type: 'progress'; value: number; color: Color }
	| { type: 'list'; items: UiNode[] }
	| {
			type: 'table';
			columns: { key: string; title: string }[];
			rows: { taskId?: number; cells: Record<string, UiNode> }[];
	  }
	| { type: 'button'; text: string; command: string; args?: unknown; confirm?: string }
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

const text = (value: unknown): string | null =>
	typeof value === 'string'
		? value.slice(0, MAX_TEXT)
		: typeof value === 'number' && Number.isFinite(value)
		? String(value)
		: null;

/**
 * Plugin UI is data, never markup: only these components, only plain strings and numbers, bounded size.
 * Whatever does not fit is dropped rather than rendered.
 */
export const sanitizeTree = (
	raw: unknown,
	context: LinkContext = NO_LINKS,
): UiNode | null => {
	let budget = MAX_NODES;
	const node = (value: any, depth: number): UiNode | null => {
		if (depth > MAX_DEPTH || budget-- <= 0) return null;
		const plain = text(value);
		if (plain !== null) return { type: 'text', text: plain, tone: 'default' };
		if (!value || typeof value !== 'object') return null;
		const children = (items: unknown) =>
			(Array.isArray(items) ? items.slice(0, MAX_ITEMS) : [])
				.map((child) => node(child, depth + 1))
				.filter((child): child is UiNode => child !== null);
		switch (value.type) {
			case 'stack':
				return {
					type: 'stack',
					direction: value.direction === 'row' ? 'row' : 'column',
					children: children(value.children),
				};
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
				return t === null
					? null
					: {
							type: 'badge',
							text: t.slice(0, 40),
							color: pick(value.color, COLORS, 'gray'),
					  };
			}
			case 'stat': {
				const label = text(value.label);
				const v = text(value.value);
				return label === null || v === null
					? null
					: {
							type: 'stat',
							label,
							value: v,
							tone: pick(value.tone, TONES, 'default'),
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
				return { type: 'list', items: children(value.items) };
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
							const cell = node(row?.cells?.[key], depth + 1);
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
				const command =
					typeof value.command === 'string'
						? value.command.slice(0, 120)
						: null;
				if (t === null || !command) return null;
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
				const confirmText =
					typeof value.confirm === 'string' ? value.confirm.trim() : '';
				const confirm = confirmText ? confirmText.slice(0, 200) : undefined;
				return {
					type: 'button',
					text: t,
					command,
					...(args !== undefined ? { args } : {}),
					...(confirm !== undefined ? { confirm } : {}),
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
						const value_ = node(item?.value, depth + 1);
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
	return node(raw, 0);
};
