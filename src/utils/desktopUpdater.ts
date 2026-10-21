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

const runCheck = async (): Promise<Update | null> => {
	const { check } = await import('@tauri-apps/plugin-updater');
	const update = await check();
	if (!update) return null;
	await update.download();
	pending = update;
	updateState.value = { status: 'ready', version: update.version };
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
	if (pending) return;
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
	updateState.value = { ...updateState.value, status: 'installing' };
	try {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('mark_update_relaunch').catch(() => {});
		await pending.install();
		await invoke('relaunch_after_update');
	} catch {
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
