export type Tone = 'default' | 'muted' | 'success' | 'warning' | 'danger';
export type Color = 'gray' | 'green' | 'yellow' | 'red' | 'blue';

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
	| { type: 'button'; text: string; command: string; args?: unknown }
	| { type: 'taskLink'; taskId: number; text: string }
	| { type: 'divider' };

const TONES: Tone[] = ['default', 'muted', 'success', 'warning', 'danger'];
const COLORS: Color[] = ['gray', 'green', 'yellow', 'red', 'blue'];
const MAX_DEPTH = 8;
const MAX_NODES = 3000;
const MAX_ITEMS = 500;
const MAX_TEXT = 1000;

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
export const sanitizeTree = (raw: unknown): UiNode | null => {
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
				).map((row: any) => {
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
					args =
						value.args === undefined
							? undefined
							: JSON.parse(JSON.stringify(value.args));
				} catch {
					return null;
				}
				return args === undefined
					? { type: 'button', text: t, command }
					: { type: 'button', text: t, command, args };
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
			default:
				return null;
		}
	};
	return node(raw, 0);
};
