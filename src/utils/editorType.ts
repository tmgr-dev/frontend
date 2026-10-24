import type { EditorType } from '@/types';
import { normalizeBlocks, type ListItem } from '@/utils/editor';

const EDITOR_TYPES: readonly EditorType[] = ['markdown', 'block', 'blockmd'];

/** Normalize a raw preferred_editor value (storage/localStorage/settings) to a valid EditorType, or null. */
export const normalizeEditorType = (raw: unknown): EditorType | null => {
	if (typeof raw !== 'string') return null;
	const value = raw.toLowerCase().trim();
	return (EDITOR_TYPES as readonly string[]).includes(value)
		? (value as EditorType)
		: null;
};

/** Human-readable editor names (with the underlying engine) for UI labels. */
export const EDITOR_LABELS: Record<EditorType, string> = {
	markdown: 'Markdown (md-editor-v3)',
	blockmd: 'Block MD (Milkdown)',
	block: 'Block (Editor.js)',
};

/** Label for the Settings select; marks the Editor.js editor as legacy. */
export const editorOptionLabel = (value: unknown): string => {
	const type = normalizeEditorType(value);
	if (!type) return String(value ?? '');
	return type === 'block'
		? `${EDITOR_LABELS[type]} — legacy`
		: EDITOR_LABELS[type];
};

export const EDITOR_SETTING_HELP =
	'Applies to new tasks and to existing tasks written in Markdown. ' +
	'Markdown and Block MD store the same Markdown text, so such tasks open in whichever of the two you pick. ' +
	'Tasks created with Block (Editor.js) keep their own format and always open in that editor.';

const parseBlocks = (json: unknown): unknown[] => {
	let value = json;
	if (typeof value === 'string') {
		try {
			value = JSON.parse(value);
		} catch {
			return [];
		}
	}
	const blocks = (value as { blocks?: unknown } | null | undefined)?.blocks;
	return Array.isArray(blocks) ? blocks : [];
};

const isFilled = (value: unknown): boolean =>
	String(value ?? '')
		.replace(/&nbsp;|<br\s*\/?>/gi, '')
		.trim() !== '';

const hasItemText = (items: ListItem[]): boolean =>
	items.some((item) => isFilled(item.content) || hasItemText(item.items));

export const hasBlockContent = (json: unknown): boolean =>
	parseBlocks(json).some((block: any) => {
		if (!block) return false;
		if (block.type === 'paragraph') return isFilled(block.data?.text);
		if (block.type !== 'list' && block.type !== 'checklist') return true;
		// Same rules as normalizeBlocks (TM-589): a stub it keeps as is stays content.
		const [list] = normalizeBlocks({ blocks: [block] }).blocks;
		if (!list) return false;
		return !Array.isArray(list.data?.items) || hasItemText(list.data.items);
	});

/** Old tasks keep HTML (not JSON) in description_json; Editor.js loads it with renderFromHTML. */
export const isLegacyHtml = (json: unknown): json is string => {
	if (typeof json !== 'string' || !json.trim()) return false;
	try {
		const parsed = JSON.parse(json);
		return parsed !== null && typeof parsed !== 'object';
	} catch {
		return true;
	}
};

export const hasMarkdownContent = (description: unknown): boolean =>
	typeof description === 'string' && description.trim() !== '';

/**
 * Stored data decides the editor. Block content (or legacy HTML) only: Editor.js. Markdown text only:
 * Markdown or Block MD (same stored text), Block MD only when preferred. Nothing stored,
 * or both fields filled (legacy dual-write), falls back to the preference.
 */
export const resolveEditorType = (
	task:
		{ description?: unknown; description_json?: unknown } | null | undefined,
	preferred: EditorType,
): EditorType => {
	const hasJson =
		hasBlockContent(task?.description_json) ||
		isLegacyHtml(task?.description_json);
	const hasText = hasMarkdownContent(task?.description);
	if (hasJson && !hasText) return 'block';
	if (hasText && !hasJson)
		return preferred === 'blockmd' ? 'blockmd' : 'markdown';
	return preferred;
};

/**
 * Description part of a task update body. Sends only the field of the active format and
 * asks the server to clear the other one, so a task never keeps two diverging copies.
 */
export const buildDescriptionPayload = (
	type: EditorType,
	task: { description?: unknown; description_json?: unknown },
): Record<string, unknown> =>
	type === 'block'
		? {
				description_json: task.description_json ?? null,
				clear_description: true,
			}
		: {
				description: task.description ?? null,
				clear_description_json: true,
			};

/** Drops the description field a queued snapshot marked as cleared, so the request depends only on the snapshot. */
export const toUpdateBody = <T extends Record<string, any>>(snapshot: T): T => {
	const body: Record<string, any> = { ...snapshot };
	if (body.clear_description === true) delete body.description;
	if (body.clear_description_json === true) delete body.description_json;
	return body as T;
};
