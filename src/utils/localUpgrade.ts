import { reactive } from 'vue';

export const LOCAL_UPGRADE_KEY = 'tmgr:local-upgrade';

export type UpgradeMarker = { migrate?: boolean; dialog?: boolean };
export type KeyOutcome = 'none' | 'moved' | 'already' | 'conflict' | 'other';
export type UpgradePhase = 'idle' | 'running' | 'done' | 'failed';

type Mutation = { type: string; payload?: unknown };

type UpgradeStore = {
	state: { user?: unknown; localProfile?: unknown };
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
}>({ phase: 'idle', key: null, dialog: false });

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

	const initial = readUpgradeMarker(storage);
	upgradeState.dialog = !!initial?.dialog;
	upgradeState.phase = initial?.migrate ? 'running' : 'idle';
	upgradeState.key = null;

	const settle = (migrate: boolean) => {
		const marker = readUpgradeMarker(storage) ?? {};
		writeUpgradeMarker({ ...marker, migrate }, storage);
	};

	const attempt = async () => {
		const current = positiveId(store.state.user);
		if (current !== lastUserId) finished = false;
		lastUserId = current;
		if (finished) return;
		const marker = readUpgradeMarker(storage);
		if (!marker?.migrate) return;
		const userId = positiveId(store.state.user);
		if (userId === null) return;
		upgradeState.phase = 'running';

		let linked: { linked_user_id?: unknown } | null;
		try {
			linked = (await invoke('local_profile_link', { userId })) as {
				linked_user_id?: unknown;
			} | null;
		} catch {
			console.error('[upgrade] could not link the local profile');
			upgradeState.phase = 'failed';
			finished = true;
			return;
		}
		if (linked) store.commit('setLocalProfile', linked);

		if (linked?.linked_user_id !== userId) {
			settle(false);
			upgradeState.key = 'other';
			upgradeState.phase = 'done';
			finished = true;
			return;
		}

		try {
			const llm = (await invoke('persona_llm_migrate_local', {
				toUserId: userId,
			})) as { status?: string } | null;
			await invoke('persona_cache_migrate_local', { toUserId: userId });
			const status = llm?.status;
			upgradeState.key =
				status === 'moved' || status === 'already' || status === 'conflict'
					? status
					: 'none';
		} catch {
			console.error('[upgrade] could not move local data to the account');
			upgradeState.phase = 'failed';
			finished = true;
			return;
		}
		settle(false);
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
		if (mutation.type === 'setUser') void schedule();
	});
	void schedule();
};
