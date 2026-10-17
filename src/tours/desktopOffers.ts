import { desktopWindowLabel, isDesktopApp } from '@/utils/desktop';
import { useTours, type ToursApi } from './useTours';

const OFFER_RETRY_MS = 60_000;
const BACKUPS_KEY = 'tmgr:tours:offered:desktop.backups';
const MOVE_KEY = 'tmgr:tours:offered:desktop.move-to-cloud';

export interface DesktopOfferEnv {
	isDesktop(): boolean;
	windowLabel(): string | null;
	storage: Pick<Storage, 'getItem' | 'setItem'>;
	offer: ToursApi['offer'];
	hasLocalWorkspaces(): Promise<boolean>;
}

const inMainDesktopWindow = (env: DesktopOfferEnv) =>
	env.isDesktop() && env.windowLabel() === 'main';

const seen = (env: DesktopOfferEnv, key: string) => {
	try {
		return env.storage.getItem(key) === '1';
	} catch {
		return false;
	}
};

const remember = (env: DesktopOfferEnv, key: string) => {
	try {
		env.storage.setItem(key, '1');
	} catch {
		return;
	}
};

const inFlight = new Map<string, Promise<void>>();

const deliver = (
	env: DesktopOfferEnv,
	key: string,
	id: 'desktop.backups' | 'desktop.move-to-cloud',
) => {
	const pending = inFlight.get(key);
	if (pending) return pending;
	const run = env
		.offer(id, 'event', { retryMs: OFFER_RETRY_MS })
		.then((shown) => {
			if (shown) remember(env, key);
		})
		.catch(() => {})
		.finally(() => inFlight.delete(key));
	inFlight.set(key, run);
	return run;
};

export function offerBackups(env: DesktopOfferEnv): void {
	if (!inMainDesktopWindow(env) || seen(env, BACKUPS_KEY)) return;
	void deliver(env, BACKUPS_KEY, 'desktop.backups');
}

export async function offerMoveToCloud(env: DesktopOfferEnv): Promise<void> {
	if (!inMainDesktopWindow(env) || seen(env, MOVE_KEY)) return;
	try {
		if (!(await env.hasLocalWorkspaces())) return;
	} catch {
		return;
	}
	await deliver(env, MOVE_KEY, 'desktop.move-to-cloud');
}

export const desktopOfferEnv = (
	hasLocalWorkspaces: () => Promise<boolean> = async () => false,
): DesktopOfferEnv => ({
	isDesktop: isDesktopApp,
	windowLabel: desktopWindowLabel,
	storage: localStorage,
	offer: (id, trigger, options) => useTours().offer(id, trigger, options),
	hasLocalWorkspaces,
});
