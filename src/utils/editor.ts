import type { Task } from '@/actions/tmgr/tasks';

export const getBlockEditorDescription = (description: Task['description']) => {
	return {
		time: 1731156298664,
		blocks: [
			{
				id: 'Sb8IPG_R7P',
				type: 'paragraph',
				data: {
					text: description,
				},
			},
		],
		version: '2.30.6',
	};
};

type ListStyle = 'ordered' | 'unordered' | 'checklist';

export interface ListItem {
	content: string;
	meta: { checked?: boolean };
	items: ListItem[];
}

export const listStart = (meta: unknown): number => {
	const start = (meta as { start?: unknown } | undefined)?.start;
	return Number.isInteger(start) && (start as number) > 0
		? (start as number)
		: 1;
};

const LIST_STYLES: ListStyle[] = ['ordered', 'unordered', 'checklist'];

// Objects are built key by key in the order @editorjs/list@2 save() emits them,
// so an untouched legacy description stringifies equal to the editor output.
const listItems = (items: unknown, style: ListStyle): ListItem[] =>
	(Array.isArray(items) ? items : []).flatMap((item) => {
		if (typeof item === 'string')
			return [
				{
					content: item,
					meta: style === 'checklist' ? { checked: false } : {},
					items: [],
				},
			];
		if (!item || typeof item !== 'object') return [];
		const content = item.content ?? item.text;
		return [
			{
				content: typeof content === 'string' ? content : '',
				meta:
					style === 'checklist'
						? { checked: (item.meta?.checked ?? item.checked) === true }
						: {},
				items: listItems(item.items, style),
			},
		];
	});

const normalizeList = (block: any) => {
	const data = block.data ?? {};
	const style: ListStyle =
		block.type === 'checklist'
			? 'checklist'
			: LIST_STYLES.includes(data.style)
			? data.style
			: 'unordered';
	const items = listItems(data.items, style);
	// D-002: list@2 throws on an empty items array, and an empty list has no content.
	if (!items.length) return null;
	const meta =
		style === 'ordered'
			? {
					start: data.meta?.start,
					counterType: data.meta?.counterType ?? 'numeric',
			  }
			: {};
	return { ...block, type: 'list', data: { style, meta, items } };
};

/** Old list@1 / nested-list / checklist blocks -> @editorjs/list@2 format. Never mutates the input. */
export const normalizeBlocks = (
	doc: unknown,
): { blocks: any[]; [key: string]: unknown } => {
	let value = doc;
	if (typeof value === 'string') {
		try {
			value = JSON.parse(value);
		} catch {
			return { blocks: [] };
		}
	}
	if (!value || typeof value !== 'object') return { blocks: [] };
	const blocks = (value as { blocks?: unknown }).blocks;
	return {
		...value,
		blocks: (Array.isArray(blocks) ? blocks : []).flatMap((block) =>
			block && (block.type === 'list' || block.type === 'checklist')
				? normalizeList(block) ?? []
				: [block],
		),
	};
};
