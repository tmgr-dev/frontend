import type { Telemetry } from './client';
import type { UsageConsent } from './consent';

export const attachTelemetry = (
	telemetry: Telemetry,
	usage: Pick<UsageConsent, 'subscribe'>,
	win: Window,
	onQuit?: (handler: () => void) => void,
): (() => void) => {
	const flushNow = () => void telemetry.flush({ keepalive: true });
	const onVisibility = () => {
		if (win.document.visibilityState === 'hidden') flushNow();
	};
	const unsubscribe = usage.subscribe((change) => {
		if (change === 'withdraw') return telemetry.withdraw();
		if (change === 'deny') telemetry.stop();
		else telemetry.sync();
	});
	win.addEventListener('pagehide', flushNow);
	win.document.addEventListener('visibilitychange', onVisibility);
	onQuit?.(flushNow);
	telemetry.sync();
	return () => {
		unsubscribe();
		win.removeEventListener('pagehide', flushNow);
		win.document.removeEventListener('visibilitychange', onVisibility);
	};
};

// Any JS close-requested listener makes Tauri block the native close; only `main` hides instead of closing.
export const onMainWindowClose = async (handler: () => void): Promise<void> => {
	const { getCurrentWindow } = await import('@tauri-apps/api/window');
	const win = getCurrentWindow();
	if (win.label !== 'main') return;
	await win.listen('tauri://close-requested', handler);
};
