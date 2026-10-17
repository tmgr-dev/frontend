export type AccountMode = 'cloud' | 'local' | 'none';

export interface SessionFlags {
	isLoggedIn: boolean;
	canUseApp: boolean;
}

export const shouldReloadOnModeChange = (
	previous: AccountMode,
	next: AccountMode,
): boolean =>
	(previous === 'local' && next === 'cloud') ||
	(previous === 'cloud' && next === 'local');

export const shouldCloseDetachedWindow = (
	previous: SessionFlags,
	next: SessionFlags,
): boolean => !next.canUseApp || (previous.isLoggedIn && !next.isLoggedIn);
