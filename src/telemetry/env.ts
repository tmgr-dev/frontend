import type { AccountMode, Platform } from './catalog';
import { majorMinor, viewportClass, type EnvelopeMeta } from './envelope';

export interface EnvDeps {
	win: Pick<Window, 'innerWidth' | 'matchMedia'>;
	desktop: 'macos' | 'windows' | 'linux' | null | false;
	appVersion: string;
	accountMode: () => AccountMode;
}

const desktopPlatformName = (
	desktop: 'macos' | 'windows' | 'linux' | null,
): Platform =>
	desktop === 'windows'
		? 'desktop_win'
		: desktop === 'linux'
		? 'desktop_linux'
		: 'desktop_mac';

export const makeMeta =
	({ win, desktop, appVersion, accountMode }: EnvDeps) =>
	(): Omit<EnvelopeMeta, 'session_id'> => {
		const viewport = viewportClass(win.innerWidth);
		const small =
			viewport === 's' && !!win.matchMedia?.('(pointer: coarse)').matches;
		return {
			platform:
				desktop === false
					? small
						? 'mobile_web'
						: 'web'
					: desktopPlatformName(desktop),
			app_version: majorMinor(appVersion),
			account_mode: accountMode(),
			viewport,
		};
	};

export const telemetryBaseUrl = (
	configured: string | undefined,
	origin: string,
): string => {
	const base = configured || '/api/';
	const url = new URL(base, origin).toString();
	return url.endsWith('/') ? url : `${url}/`;
};
