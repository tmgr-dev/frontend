import { normalizeEditorType } from '@/utils/editorType';

describe('normalizeEditorType', () => {
	it('accepts blockmd', () => {
		expect(normalizeEditorType('blockmd')).toBe('blockmd');
	});

	it('accepts legacy markdown and block', () => {
		expect(normalizeEditorType('markdown')).toBe('markdown');
		expect(normalizeEditorType('block')).toBe('block');
	});

	it('normalizes case and whitespace', () => {
		expect(normalizeEditorType('  BlockMD ')).toBe('blockmd');
	});

	it('returns null for unknown, empty, or non-string values', () => {
		expect(normalizeEditorType('quill')).toBeNull();
		expect(normalizeEditorType('')).toBeNull();
		expect(normalizeEditorType(null)).toBeNull();
		expect(normalizeEditorType(undefined)).toBeNull();
		expect(normalizeEditorType(42)).toBeNull();
	});
});
