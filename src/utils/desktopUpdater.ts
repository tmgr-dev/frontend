import type { Update } from '@tauri-apps/plugin-updater';
import { ref } from 'vue';

type UpdateStatus = 'idle' | 'checking' | 'ready' | 'installing';

export const updateState = ref<{ status: UpdateStatus; version: string }>({
	status: 'idle',
	version: '',
});

export type ManualCheckOutcome =
	| { kind: 'up-to-date' }
	| { kind: 'available'; version: string }
	| { kind: 'error'; message: string };

let pending: Update | null = null;
let inFlight: Promise<Update | null> | null = null;
let installing = false;

const runCheck = async (): Promise<Update | null> => {
	const { check } = await import('@tauri-apps/plugin-updater');
	const update = await check();
	if (!update) return pending;
	if (pending && pending.version === update.version) {
		await update.close().catch(() => {});
		return pending;
	}
	try {
		await update.download();
	} catch (err) {
		await update.close().catch(() => {});
		throw err;
	}
	const previous = pending;
	pending = update;
	updateState.value = {
		status: installing ? 'installing' : 'ready',
		version: update.version,
	};
	await previous?.close?.().catch(() => {});
	return update;
};

// Single-flight: the auto check and every manual click (macOS emits the menu event twice) share one run.
const sharedCheck = (): Promise<Update | null> => {
	if (!inFlight) {
		inFlight = runCheck().finally(() => {
			inFlight = null;
		});
	}
	return inFlight;
};

const failureMessage = (err: unknown): string =>
	err instanceof Error ? err.message : String(err);

const log = async (level: 'error' | 'warn', message: string) => {
	try {
		const plugin = await import('@tauri-apps/plugin-log');
		await plugin[level](message);
	} catch {
		/* logging must never break the update check */
	}
};

export async function checkForUpdate(): Promise<void> {
	if (installing) return;
	try {
		await sharedCheck();
	} catch (err) {
		await log(
			'warn',
			`[updater] background check failed: ${failureMessage(err)}`,
		);
	}
}

export async function checkForUpdateManually(): Promise<ManualCheckOutcome> {
	if (pending) return { kind: 'available', version: pending.version };
	updateState.value = { status: 'checking', version: '' };
	try {
		const update = await sharedCheck();
		if (update) return { kind: 'available', version: update.version };
		updateState.value = { status: 'idle', version: '' };
		return { kind: 'up-to-date' };
	} catch (err) {
		updateState.value = { status: 'idle', version: '' };
		const message = failureMessage(err);
		await log('error', `[updater] manual check failed: ${message}`);
		return { kind: 'error', message };
	}
}

export async function installUpdate(): Promise<void> {
	if (!pending || updateState.value.status !== 'ready') return;
	installing = true;
	let installed = false;
	updateState.value = { ...updateState.value, status: 'installing' };
	try {
		await sharedCheck().catch(() => {});
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('mark_update_relaunch').catch(() => {});
		await pending!.install();
		installed = true;
		await invoke('relaunch_after_update');
	} catch {
		installing = false;
		if (installed) {
			pending = null;
			updateState.value = { status: 'idle', version: '' };
			return;
		}
		updateState.value = { ...updateState.value, status: 'ready' };
	}
}

export function startUpdateChecks(
	firstDelayMs = 10_000,
	intervalMs = 6 * 60 * 60 * 1000,
): void {
	setTimeout(checkForUpdate, firstDelayMs);
	setInterval(checkForUpdate, intervalMs);
}
