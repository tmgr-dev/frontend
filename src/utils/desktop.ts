import { t } from '@/i18n';
export const isDesktopApp = (): boolean =>
	typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const desktopWindowLabel = (): string | null =>
	(window as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? null;

export type DesktopPlatform = 'macos' | 'windows' | 'linux';

export const desktopPlatform = (
	nav:
		{ platform?: string; userAgent?: string } | undefined = typeof navigator ===
	'undefined'
		? undefined
		: navigator,
): DesktopPlatform | null => {
	const hint = `${nav?.platform ?? ''} ${nav?.userAgent ?? ''}`;
	if (/mac|darwin/i.test(hint)) return 'macos';
	if (/win/i.test(hint)) return 'windows';
	if (/linux|x11|cros/i.test(hint)) return 'linux';
	return null;
};

export const isMacLike = (platform: DesktopPlatform | null): boolean =>
	platform === 'macos' || platform === null;

export const hasNativeTitleBar = (
	platform: DesktopPlatform | null = desktopPlatform(),
): boolean => platform === 'windows' || platform === 'linux';

export const trayName = (
	platform: DesktopPlatform | null = desktopPlatform(),
): string =>
	t(
		isMacLike(platform)
			? 'desktop.helpers.trayName.menuBar'
			: 'desktop.helpers.trayName.systemTray',
	);

export const revealInFileManagerLabel = (
	platform: DesktopPlatform | null = desktopPlatform(),
): string => {
	if (platform === 'windows')
		return t('desktop.helpers.revealInFileManager.explorer');
	if (platform === 'linux')
		return t('desktop.helpers.revealInFileManager.fileManager');
	return t('desktop.helpers.revealInFileManager.finder');
};

export const thisComputerName = (
	platform: DesktopPlatform | null = desktopPlatform(),
): string =>
	t(
		isMacLike(platform)
			? 'desktop.helpers.thisComputer.mac'
			: 'desktop.helpers.thisComputer.computer',
	);

export const supportsScreenCapture = (
	platform: DesktopPlatform | null = desktopPlatform(),
): boolean => isMacLike(platform);

export const supportsSelectionCapture = (
	platform: DesktopPlatform | null = desktopPlatform(),
): boolean => isMacLike(platform);

export const pluginWindowsUnavailable = (): string =>
	t('desktop.helpers.pluginWindowsUnavailable');

export const supportsPluginWindows = (
	platform: DesktopPlatform | null = desktopPlatform(),
): boolean => platform !== 'windows';

export const supportsLocalAccess = (
	platform: DesktopPlatform | null = desktopPlatform(),
): boolean => platform !== 'windows';

export function installAutoHideScrollbars(
	doc: Document = document,
	hideAfterMs = 900,
): void {
	const timers = new WeakMap<Element, ReturnType<typeof setTimeout>>();
	doc.addEventListener(
		'scroll',
		(event) => {
			const el =
				event.target instanceof Element ? event.target : doc.documentElement;
			el.classList.add('is-scrolling');
			clearTimeout(timers.get(el));
			timers.set(
				el,
				setTimeout(() => el.classList.remove('is-scrolling'), hideAfterMs),
			);
		},
		{ capture: true, passive: true },
	);
}
