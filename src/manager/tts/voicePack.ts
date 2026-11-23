import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { reactive } from 'vue';

interface Status {
	supported: boolean;
	installed: boolean;
	installing: boolean;
	sizeBytes: number;
	sampleRate: number;
}

interface Progress {
	received: number;
	total: number;
}

const PROGRESS_EVENT = 'manager://tts-progress';
const CANCELLED = /cancel/i;

const apply = (s: Status): void => {
	voicePack.supported = s.supported;
	voicePack.installed = s.installed;
	voicePack.installing = s.installing;
	voicePack.sizeBytes = s.sizeBytes;
	voicePack.sampleRate = s.sampleRate;
};

const refresh = async (): Promise<void> => {
	try {
		apply(await invoke<Status>('tts_status'));
	} catch {
		voicePack.supported = false;
		voicePack.installed = false;
		voicePack.installing = false;
	}
};

const install = async (): Promise<void> => {
	if (voicePack.installing) return;
	voicePack.installing = true;
	voicePack.progress = 0;
	voicePack.error = null;
	const unlisten = await listen<Progress>(PROGRESS_EVENT, (e) => {
		const { received, total } = e.payload;
		voicePack.progress = total > 0 ? received / total : 0;
	}).catch(() => undefined);
	try {
		await invoke('tts_install');
	} catch (e) {
		const reason = String(e);
		if (!CANCELLED.test(reason)) voicePack.error = reason;
	} finally {
		unlisten?.();
		voicePack.installing = false;
		await refresh();
	}
};

const cancel = async (): Promise<void> => {
	try {
		await invoke('tts_cancel_install');
	} catch (e) {
		voicePack.error = String(e);
	}
	await refresh();
};

const remove = async (): Promise<void> => {
	try {
		await invoke('tts_remove');
	} catch (e) {
		voicePack.error = String(e);
	}
	await refresh();
};

export const voicePack = reactive({
	supported: false,
	installed: false,
	installing: false,
	sizeBytes: 0,
	sampleRate: 0,
	progress: 0,
	error: null as string | null,
	statusRequested: false,
	refresh,
	install,
	cancel,
	remove,
});
