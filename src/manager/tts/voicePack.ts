import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { reactive } from 'vue';

export type PackLang = 'en' | 'ru';

interface Status {
	supported: boolean;
	installed: boolean;
	installing: boolean;
	sizeBytes: number;
	sampleRate: number;
}

interface Progress {
	lang?: PackLang;
	received: number;
	total: number;
}

const PROGRESS_EVENT = 'manager://tts-progress';
const CANCELLED = /cancel/i;

const createPack = (lang: PackLang) => {
	const apply = (s: Status): void => {
		pack.supported = s.supported;
		pack.installed = s.installed;
		pack.installing = s.installing;
		pack.sizeBytes = s.sizeBytes;
		pack.sampleRate = s.sampleRate;
	};

	const refresh = async (): Promise<void> => {
		try {
			apply(await invoke<Status>('tts_status', { lang }));
		} catch {
			pack.supported = false;
			pack.installed = false;
			pack.installing = false;
		}
	};

	const install = async (): Promise<void> => {
		if (pack.installing) return;
		pack.installing = true;
		pack.progress = 0;
		pack.error = null;
		const unlisten = await listen<Progress>(PROGRESS_EVENT, (e) => {
			const { received, total } = e.payload;
			if ((e.payload.lang ?? 'en') !== lang) return;
			pack.progress = total > 0 ? received / total : 0;
		}).catch(() => undefined);
		try {
			await invoke('tts_install', { lang });
		} catch (e) {
			const reason = String(e);
			if (!CANCELLED.test(reason)) pack.error = reason;
		} finally {
			unlisten?.();
			pack.installing = false;
			await refresh();
		}
	};

	const cancel = async (): Promise<void> => {
		try {
			await invoke('tts_cancel_install', { lang });
		} catch (e) {
			pack.error = String(e);
		}
		await refresh();
	};

	const remove = async (): Promise<void> => {
		try {
			await invoke('tts_remove', { lang });
		} catch (e) {
			pack.error = String(e);
		}
		await refresh();
	};

	const pack = reactive({
		lang,
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
	return pack;
};

export const voicePacks = { en: createPack('en'), ru: createPack('ru') };

export const voicePack = voicePacks.en;
