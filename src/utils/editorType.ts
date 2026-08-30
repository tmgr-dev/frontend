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
