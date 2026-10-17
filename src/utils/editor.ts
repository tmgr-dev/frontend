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

// Unreadable values become visible text instead of being lost (TM-591 D-005);
// escaped because list@2 sets content via innerHTML.
const asText = (value: unknown): string =>
	value && typeof value === 'object'
		? JSON.stringify(value)
				.replace(/&/g, '&amp;')
				.replace(/</g, '&lt;')
				.replace(/>/g, '&gt;')
		: String(value);

const ITEM_KEYS = ['content', 'text', 'meta', 'checked', 'items'];

const itemContent = (item: any): string => {
	const { content, text } = item;
	if (content === '' && text != null && text !== '')
		return itemContent({ text });
	if (typeof content === 'string') return content;
	if (typeof text === 'string') return text;
	if (Number.isFinite(content)) return String(content);
	if (Number.isFinite(text)) return String(text);
	if (content == null && text == null) {
		const extra = Object.fromEntries(
			Object.entries(item).filter(
				([key, value]) => value != null && !ITEM_KEYS.includes(key),
			),
		);
		return Object.keys(extra).length ? asText(extra) : '';
	}
	return asText(content ?? text);
};

// Objects are built key by key in the order @editorjs/list@2 save() emits them,
// so an untouched legacy description stringifies equal to the editor output.
const listItems = (items: any[], style: ListStyle): ListItem[] =>
	items.flatMap((item) => {
		if (item == null) return [];
		const meta =
			style === 'checklist'
				? { checked: (item.meta?.checked ?? item.checked) === true }
				: {};
		if (typeof item !== 'object' || Array.isArray(item))
			return [{ content: asText(item), meta, items: [] }];
		const children =
			item.items == null
				? []
				: Array.isArray(item.items)
				? item.items
				: [item.items];
		return [
			{
				content: itemContent(item),
				meta,
				items: listItems(children, style),
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
	// list@2 throws on non-array items, so Editor.js keeps such a block as a stub
	// and saves it back unchanged; normalising it could only lose data (TM-591 D-005).
	if (
		(block.data != null &&
			(typeof block.data !== 'object' || Array.isArray(block.data)) &&
			Object.keys(block.data).length > 0) ||
		(data.items != null && !Array.isArray(data.items))
	)
		return block;
	const items = listItems(data.items ?? [], style);
	// TM-582 D-002: list@2 throws on an empty items array, and an empty list has no content.
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

const sortedKeys = (_key: string, value: unknown) =>
	value && typeof value === 'object' && !Array.isArray(value)
		? Object.fromEntries(
				Object.keys(value)
					.sort()
					.map((key) => [key, (value as Record<string, unknown>)[key]]),
		  )
		: value;

/** Saved Editor.js blocks equal the stored document, ignoring key order (a MySQL JSON column reorders keys). */
export const sameBlocks = (blocks: unknown, doc: unknown) =>
	JSON.stringify(blocks, sortedKeys) ===
	JSON.stringify(normalizeBlocks(doc).blocks, sortedKeys);
