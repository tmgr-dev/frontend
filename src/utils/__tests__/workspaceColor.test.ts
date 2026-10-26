import { contrastRatio } from '@/theme/paletteCss';
import {
	WORKSPACE_COLORS,
	normalizeWorkspaceColor,
	workspaceIconStyle,
} from '@/utils/workspaceColor';

describe('workspaceIconStyle', () => {
	test.each([
		undefined,
		null,
		'',
		'   ',
		'red',
		'#fff',
		'#12345',
		'javascript:1',
	])('no inline style for %p', (color) => {
		expect(workspaceIconStyle(color)).toBeUndefined();
	});

	test('picks black on light and white on dark backgrounds', () => {
		expect(workspaceIconStyle('#EAB308')).toEqual({
			backgroundColor: '#EAB308',
			color: '#000000',
		});
		expect(workspaceIconStyle('#1e3a8a')).toEqual({
			backgroundColor: '#1E3A8A',
			color: '#ffffff',
		});
	});

	test('every palette swatch gets the higher-contrast foreground, at least 3:1', () => {
		for (const { value } of WORKSPACE_COLORS) {
			const style = workspaceIconStyle(value)!;
			const other = style.color === '#000000' ? '#ffffff' : '#000000';
			const ratio = contrastRatio(style.color, value);
			expect(ratio).toBeGreaterThanOrEqual(contrastRatio(other, value));
			expect(ratio).toBeGreaterThanOrEqual(3);
		}
	});

	test('normalizeWorkspaceColor upper-cases valid hex and drops the rest', () => {
		expect(normalizeWorkspaceColor(' #3b82f6 ')).toBe('#3B82F6');
		expect(normalizeWorkspaceColor('blue')).toBe('');
		expect(normalizeWorkspaceColor(null)).toBe('');
	});
});
