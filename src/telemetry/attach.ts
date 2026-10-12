import { CONSENT_KEY, type ConsentStore } from '@/privacy/consent';
import type { Telemetry } from './client';
import { IDENTITY_KEY } from './identity';

export const attachTelemetry = (
	telemetry: Telemetry,
	store: ConsentStore,
	win: Window,
	onQuit?: (handler: () => void) => void,
): (() => void) => {
	const flushNow = () => void telemetry.flush({ keepalive: true });
	const onStorage = (event: StorageEvent) => {
		if (event.key === null || event.key === CONSENT_KEY || event.key === IDENTITY_KEY) {
			telemetry.sync();
		}
	};
	const onVisibility = () => {
		if (win.document.visibilityState === 'hidden') flushNow();
	};
	const unsubscribe = store.subscribe(() => telemetry.sync());
	win.addEventListener('storage', onStorage);
	win.addEventListener('pagehide', flushNow);
	win.document.addEventListener('visibilitychange', onVisibility);
	onQuit?.(flushNow);
	telemetry.sync();
	return () => {
		unsubscribe();
		win.removeEventListener('storage', onStorage);
		win.removeEventListener('pagehide', flushNow);
		win.document.removeEventListener('visibilitychange', onVisibility);
	};
};
