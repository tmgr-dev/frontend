import { t } from '@/i18n';
import { reactive } from 'vue';
import { isValidCode } from './code';
import type { PairingView, PhoneView } from './types';

export interface PhonesApi {
	list: () => Promise<PhoneView[]>;
	pairing: () => Promise<PairingView | null>;
	confirm: (code: string) => Promise<void>;
	cancel: () => Promise<void>;
	revoke: (id: string) => Promise<void>;
}

const messageOf = (error: unknown): string =>
	typeof error === 'string' && error
		? error
		: ((error as any)?.message ??
			t('settingsPages.helpers.phones.genericError'));

export const createPhonesController = (api: PhonesApi) => {
	const state = reactive({
		phones: [] as PhoneView[],
		pairing: null as PairingView | null,
		revokeId: null as string | null,
		error: '' as string,
		busy: false,
	});

	const refresh = async () => {
		try {
			const [phones, pairing] = await Promise.all([api.list(), api.pairing()]);
			state.phones = phones;
			state.pairing = pairing;
			if (state.revokeId && !phones.some((p) => p.id === state.revokeId)) {
				state.revokeId = null;
			}
		} catch (error) {
			state.error = messageOf(error);
		}
	};

	const run = async (action: () => Promise<void>) => {
		state.busy = true;
		state.error = '';
		try {
			await action();
		} catch (error) {
			state.error = messageOf(error);
		} finally {
			state.busy = false;
		}
		await refresh();
	};

	const askRevoke = (id: string) => {
		state.revokeId = id;
	};

	const cancelRevoke = () => {
		state.revokeId = null;
	};

	const confirmRevoke = async () => {
		const id = state.revokeId;
		if (!id) return;
		state.revokeId = null;
		await run(() => api.revoke(id));
	};

	const submitCode = async (code: string) => {
		if (!isValidCode(code) || state.pairing?.state !== 'confirm') return;
		await run(() => api.confirm(code));
	};

	const cancelPairing = () => run(() => api.cancel());

	return {
		state,
		refresh,
		askRevoke,
		cancelRevoke,
		confirmRevoke,
		submitCode,
		cancelPairing,
	};
};
