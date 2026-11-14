export const isTaskActivity = (subjectType: string | null | undefined) =>
	subjectType?.toLowerCase() === 'task';

export const formatRoutineRate = (percent: number): string =>
	`${Math.round(percent)}%`;

export const isTypingTarget = (target: EventTarget | null): boolean => {
	const el = target as HTMLElement | null;
	if (!el || typeof el.closest !== 'function') return false;
	return !!el.closest(
		'input, textarea, select, [contenteditable]:not([contenteditable="false"])',
	);
};

export const modifierLabel = (isMac: boolean): string => (isMac ? '⌘' : 'Ctrl');
