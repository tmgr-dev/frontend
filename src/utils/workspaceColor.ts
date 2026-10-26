import { contrastRatio } from '@/theme/paletteCss';

export const WORKSPACE_COLORS = [
	{ value: '#EF4444', name: 'Red' },
	{ value: '#F97316', name: 'Orange' },
	{ value: '#EAB308', name: 'Yellow' },
	{ value: '#22C55E', name: 'Green' },
	{ value: '#14B8A6', name: 'Teal' },
	{ value: '#3B82F6', name: 'Blue' },
	{ value: '#8B5CF6', name: 'Violet' },
	{ value: '#EC4899', name: 'Pink' },
] as const;

const HEX = /^#[0-9a-f]{6}$/i;

export const normalizeWorkspaceColor = (color?: string | null): string =>
	typeof color === 'string' && HEX.test(color.trim())
		? color.trim().toUpperCase()
		: '';

export const workspaceIconStyle = (
	color?: string | null,
): { backgroundColor: string; color: string } | undefined => {
	const bg = normalizeWorkspaceColor(color);
	if (!bg) return undefined;
	return {
		backgroundColor: bg,
		color:
			contrastRatio('#000000', bg) >= contrastRatio('#ffffff', bg)
				? '#000000'
				: '#ffffff',
	};
};
