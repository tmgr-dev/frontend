import type { EditorType } from '@/types';

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
