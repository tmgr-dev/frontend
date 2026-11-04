import { reactive } from 'vue';

export const LOCAL_UPGRADE_KEY = 'tmgr:local-upgrade';

export type UpgradeMarker = { migrate?: boolean; dialog?: boolean };
export type KeyOutcome = 'none' | 'moved' | 'already' | 'conflict' | 'other';
export type UpgradePhase = 'idle' | 'running' | 'done' | 'failed';
export type FailedStep = 'link' | 'move';

type Mutation = { type: string; payload?: unknown };

type UpgradeStore = {
	state: { user?: unknown; token?: unknown; localProfile?: unknown };
	subscribe: (fn: (mutation: Mutation) => void) => unknown;
	commit: (type: string, payload?: unknown) => void;
};

type Invoke = (
	command: string,
	args?: Record<string, unknown>,
) => Promise<unknown>;

type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

export const upgradeState = reactive<{
	phase: UpgradePhase;
	key: KeyOutcome | null;
	dialog: boolean;
	failed: FailedStep | null;
}>({ phase: 'idle', key: null, dialog: false, failed: null });

const defaultStorage = (): StorageLike | null => {
	try {
		return localStorage;
	} catch {
		return null;
	}
};

const defaultInvoke: Invoke = async (command, args) => {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke(command, args);
};

export const readUpgradeMarker = (
	storage: StorageLike | null = defaultStorage(),
): UpgradeMarker | null => {
	try {
		const raw = storage?.getItem(LOCAL_UPGRADE_KEY);
		if (!raw) return null;
		const parsed = JSON.parse(raw);
		if (!parsed || typeof parsed !== 'object') return null;
		return {
			migrate: parsed.migrate === true,
			dialog: parsed.dialog === true,
		};
	} catch {
		return null;
	}
};

const writeUpgradeMarker = (
	marker: UpgradeMarker,
	storage: StorageLike | null,
) => {
	try {
		if (!marker.migrate && !marker.dialog)
			storage?.removeItem(LOCAL_UPGRADE_KEY);
		else storage?.setItem(LOCAL_UPGRADE_KEY, JSON.stringify(marker));
	} catch {
		/* storage unavailable: the upgrade simply does not run */
	}
};

export const markLocalUpgrade = (
	storage: StorageLike | null = defaultStorage(),
) => writeUpgradeMarker({ migrate: true, dialog: true }, storage);

export const clearUpgradeDialog = (
	storage: StorageLike | null = defaultStorage(),
) => {
	const marker = readUpgradeMarker(storage);
	upgradeState.dialog = false;
	if (marker) writeUpgradeMarker({ ...marker, dialog: false }, storage);
};

export const upgradeFailureSentence = (failed: FailedStep | null) =>
	failed === 'link'
		? 'This device could not be linked to your account, so your local data stays on this device for now. You can keep working, retry, or skip.'
		: 'Some local data could not be moved to this account yet. You can keep working, and we will try again next time you start the app.';

export const canSkipLocalUpgrade = (failed: FailedStep | null) =>
	failed === 'link';

export const skipLocalUpgrade = (
	storage: StorageLike | null = defaultStorage(),
) => {
	if (!canSkipLocalUpgrade(upgradeState.failed)) return;
	writeUpgradeMarker({}, storage);
	upgradeState.dialog = false;
	upgradeState.phase = 'idle';
	upgradeState.failed = null;
};

const positiveId = (user: unknown): number | null => {
	const id = Number((user as { id?: unknown } | null | undefined)?.id);
	return Number.isSafeInteger(id) && id > 0 ? id : null;
};

let activeRetry: (() => Promise<void>) | null = null;

export const retryLocalUpgrade = (): Promise<void> =>
	activeRetry ? activeRetry() : Promise.resolve();

export const installLocalUpgrade = (
	store: UpgradeStore,
	invoke: Invoke = defaultInvoke,
	storage: StorageLike | null = defaultStorage(),
) => {
	let queue: Promise<void> = Promise.resolve();
	let finished = false;
	let lastUserId: number | null = null;
	let userMatchesToken = true;

	const initial = readUpgradeMarker(storage);
	upgradeState.dialog = !!initial?.dialog;
	upgradeState.phase = initial?.migrate ? 'running' : 'idle';
	upgradeState.key = null;
	upgradeState.failed = null;

	const settle = (migrate: boolean) => {
		const marker = readUpgradeMarker(storage) ?? {};
		writeUpgradeMarker({ ...marker, migrate }, storage);
	};

	const attempt = async () => {
		const current = positiveId(store.state.user);
		if (current !== lastUserId) {
			finished = false;
			if (lastUserId !== null) {
				upgradeState.phase = 'idle';
				upgradeState.key = null;
				upgradeState.failed = null;
			}
		}
		lastUserId = current;
		if (finished) return;
		const marker = readUpgradeMarker(storage);
		if (!marker?.migrate) return;
		const userId = positiveId(store.state.user);
		if (userId === null || store.state.token == null || !userMatchesToken)
			return;
		upgradeState.phase = 'running';
		upgradeState.key = null;
		upgradeState.failed = null;

		const stale = () =>
			store.state.token == null ||
			!userMatchesToken ||
			positiveId(store.state.user) !== userId;
		const abandon = () => {
			upgradeState.phase = 'idle';
			upgradeState.key = null;
		};
		const fail = (step: FailedStep) => {
			if (stale()) return abandon();
			upgradeState.failed = step;
			upgradeState.phase = 'failed';
			finished = true;
		};

		let linked: { linked_user_id?: unknown } | null;
		try {
			linked = (await invoke('local_profile_link', { userId })) as {
				linked_user_id?: unknown;
			} | null;
		} catch {
			console.error('[upgrade] could not link the local profile');
			return fail('link');
		}
		if (linked) store.commit('setLocalProfile', linked);
		if (stale()) return abandon();

		if (linked?.linked_user_id !== userId) {
			upgradeState.key = 'other';
			upgradeState.phase = 'done';
			finished = true;
			return;
		}

		let status: string | undefined;
		try {
			await invoke('persona_cache_migrate_local', { toUserId: userId });
			if (stale()) return abandon();
			const llm = (await invoke('persona_llm_migrate_local', {
				toUserId: userId,
			})) as { status?: string } | null;
			status = llm?.status;
		} catch {
			console.error('[upgrade] could not move local data to the account');
			return fail('move');
		}
		settle(false);
		if (stale()) return abandon();
		upgradeState.key =
			status === 'moved' || status === 'already' || status === 'conflict'
				? status
				: 'none';
		upgradeState.phase = 'done';
		finished = true;
	};

	const schedule = () => {
		queue = queue.then(attempt, attempt);
		return queue;
	};

	activeRetry = () => {
		finished = false;
		return schedule();
	};

	store.subscribe((mutation) => {
		if (mutation.type === 'setToken') userMatchesToken = false;
		if (mutation.type === 'setUser') {
			userMatchesToken = true;
			void schedule();
		}
	});
	void schedule();
};
