import type { ViewBadgeEntry } from '@/pluginSystem/host';

type BadgeShape = Pick<ViewBadgeEntry, 'count' | 'text' | 'tone'>;

const MAX_COUNT = 99;

export const BADGE_TONE_CLASS: Record<ViewBadgeEntry['tone'], string> = {
	default: 'bg-sidebar-accent text-sidebar-foreground',
	info: 'bg-blue-500/15 text-blue-700 dark:bg-blue-400/20 dark:text-blue-300',
	warning:
		'bg-amber-500/15 text-amber-700 dark:bg-amber-400/20 dark:text-amber-300',
	danger: 'bg-red-500/15 text-red-700 dark:bg-red-400/20 dark:text-red-300',
};

export const BADGE_DOT_CLASS: Record<ViewBadgeEntry['tone'], string> = {
	default: 'bg-muted-foreground',
	info: 'bg-blue-500 dark:bg-blue-400',
	warning: 'bg-amber-500 dark:bg-amber-400',
	danger: 'bg-red-500 dark:bg-red-400',
};

export const viewBadgeLabel = (badge: BadgeShape): string => {
	if (badge.count !== null) {
		return badge.count > MAX_COUNT ? `${MAX_COUNT}+` : String(badge.count);
	}
	return badge.text ?? '';
};

export const viewBadgeAriaLabel = (
	title: string,
	badge: BadgeShape | null | undefined,
): string | undefined => {
	if (!badge) return undefined;
	if (badge.count !== null) return `${title}, ${badge.count} pending`;
	return badge.text ? `${title}, ${badge.text}` : undefined;
};
