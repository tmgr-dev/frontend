import { invoke } from '@tauri-apps/api/core';
import { reactive } from 'vue';
import { openPlayback } from './audioPlayback';
import { decodeExternal } from './externalEngine';

export type ExternalMode = 'off' | 'http' | 'command';

export interface ExternalSettings {
	mode: ExternalMode;
	http: {
		baseUrl: string;
		model: string;
		voice: string;
		allowRemote: boolean;
	};
	command: { argv: string[]; timeoutSecs: number };
}

export interface ExternalHealth {
	configured: boolean;
	mode: ExternalMode;
	healthy: boolean;
	lastError: string | null;
}

const CANCELLED = /cancel/i;

export const defaultExternalSettings = (): ExternalSettings => ({
	mode: 'off',
	http: { baseUrl: '', model: '', voice: '', allowRemote: false },
	command: { argv: [], timeoutSecs: 30 },
});

const refreshHealth = async (): Promise<void> => {
	try {
		store.health = await invoke<ExternalHealth>('tts_external_health');
	} catch {
		store.health = null;
	}
};

const load = async (): Promise<void> => {
	store.loading = true;
	try {
		store.settings = await invoke<ExternalSettings>('tts_settings_get');
	} catch {
		store.settings = defaultExternalSettings();
	} finally {
		store.loading = false;
	}
	await refreshHealth();
};

const save = async (settings: ExternalSettings): Promise<boolean> => {
	store.saving = true;
	store.saveError = null;
	try {
		store.settings = await invoke<ExternalSettings>('tts_settings_set', {
			settings,
		});
		await refreshHealth();
		return true;
	} catch (e) {
		const reason = String(e);
		if (!CANCELLED.test(reason)) store.saveError = reason;
		return false;
	} finally {
		store.saving = false;
	}
};

const markUnhealthy = (): void => {
	if (store.health) store.health = { ...store.health, healthy: false };
	void refreshHealth();
};

const test = async (phrase?: string): Promise<void> => {
	if (store.testing) return;
	store.testing = true;
	store.testError = null;
	let playback: ReturnType<typeof openPlayback> | null = null;
	let unavailable: unknown = null;
	try {
		playback = openPlayback(() => {
			store.testing = false;
		});
	} catch (e) {
		unavailable = e;
	}
	try {
		await playback?.ready;
		const decoded = decodeExternal(
			await invoke<ArrayBuffer>('tts_external_test', { text: phrase }),
		);
		if (decoded && !playback) throw unavailable;
		if (decoded && playback) {
			playback.schedule(decoded.samples, decoded.sampleRate);
			playback.finish();
		} else {
			store.testing = false;
		}
	} catch (e) {
		playback?.stop();
		store.testError = String(e);
		store.testing = false;
	}
	await refreshHealth();
};

const store = reactive({
	settings: defaultExternalSettings(),
	health: null as ExternalHealth | null,
	loading: false,
	saving: false,
	testing: false,
	saveError: null as string | null,
	testError: null as string | null,
	get usable(): boolean {
		const h = store.health;
		return !!h && h.mode !== 'off' && h.configured && h.healthy;
	},
	load,
	save,
	test,
	refreshHealth,
	markUnhealthy,
});

export const externalVoice = store;
