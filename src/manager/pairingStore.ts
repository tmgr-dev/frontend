import { reactive } from 'vue';
import * as realApi from './api';
import type { ManagerMachine, PairResult } from './types';

export interface PairingApi {
	pair: () => Promise<PairResult | null>;
	machines: () => Promise<ManagerMachine[]>;
}

export interface RunnerNotice {
	deviceId: string;
	label: string;
}

export const createPairingStore = (api: PairingApi) => {
	const store = reactive({
		machines: [] as ManagerMachine[],
		notice: null as RunnerNotice | null,
		blockers: [] as string[],

		async refreshMachines() {
			try {
				store.machines = await api.machines();
			} catch {
				return;
			}
		},

		async start() {
			store.notice = null;
			store.blockers = [];
			await store.refreshMachines();
			let result: PairResult | null = null;
			try {
				result = await api.pair();
			} catch (e) {
				console.warn('manager_pair_local_runner failed', e);
				return;
			}
			if (!result) return;
			if (result.state === 'paired' && result.device_id) {
				const machine = store.machines.find(
					(m) => m.machine === result.machine,
				);
				store.notice = {
					deviceId: result.device_id,
					label: machine?.label || result.machine || 'this computer',
				};
				await store.refreshMachines();
			} else if (result.state === 'blocked') {
				store.blockers = result.blockers ?? [];
			} else if (['no_cli', 'no_runner', 'error'].includes(result.state)) {
				console.warn(
					'local runner pairing:',
					result.state,
					result.message ?? '',
				);
			}
		},

		dismiss() {
			store.notice = null;
		},

		watch(intervalMs: number): () => void {
			const timer = setInterval(() => void store.refreshMachines(), intervalMs);
			return () => clearInterval(timer);
		},
	});
	return store;
};

export type PairingStore = ReturnType<typeof createPairingStore>;

let singleton: PairingStore | null = null;

export const usePairingStore = (): PairingStore => {
	singleton ??= createPairingStore({
		pair: realApi.managerPairLocalRunner,
		machines: realApi.managerMachines,
	});
	return singleton;
};
