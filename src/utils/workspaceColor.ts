import { contrastRatio } from '@/theme/paletteCss';

export const WORKSPACE_COLORS = [
	{ value: '#EF4444', nameKey: 'settingsPages.helpers.workspaceColor.red' },
	{ value: '#F97316', nameKey: 'settingsPages.helpers.workspaceColor.orange' },
	{ value: '#EAB308', nameKey: 'settingsPages.helpers.workspaceColor.yellow' },
	{ value: '#22C55E', nameKey: 'settingsPages.helpers.workspaceColor.green' },
	{ value: '#14B8A6', nameKey: 'settingsPages.helpers.workspaceColor.teal' },
	{ value: '#3B82F6', nameKey: 'settingsPages.helpers.workspaceColor.blue' },
	{ value: '#8B5CF6', nameKey: 'settingsPages.helpers.workspaceColor.violet' },
	{ value: '#EC4899', nameKey: 'settingsPages.helpers.workspaceColor.pink' },
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
