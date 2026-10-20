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
	| { kind: 'error' };

let pending: Update | null = null;
let checking = false;

export async function checkForUpdate(): Promise<void> {
	if (pending || checking) return;
	checking = true;
	try {
		const { check } = await import('@tauri-apps/plugin-updater');
		const update = await check();
		if (!update) return;
		await update.download();
		pending = update;
		updateState.value = { status: 'ready', version: update.version };
	} catch {
		/* offline or no release yet: try again on the next tick */
	} finally {
		checking = false;
	}
}

export async function checkForUpdateManually(): Promise<ManualCheckOutcome> {
	if (pending) return { kind: 'available', version: pending.version };
	if (checking) return { kind: 'error' };
	checking = true;
	updateState.value = { status: 'checking', version: '' };
	try {
		const { check } = await import('@tauri-apps/plugin-updater');
		const update = await check();
		if (!update) {
			updateState.value = { status: 'idle', version: '' };
			return { kind: 'up-to-date' };
		}
		await update.download();
		pending = update;
		updateState.value = { status: 'ready', version: update.version };
		return { kind: 'available', version: update.version };
	} catch {
		updateState.value = { status: 'idle', version: '' };
		return { kind: 'error' };
	} finally {
		checking = false;
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
