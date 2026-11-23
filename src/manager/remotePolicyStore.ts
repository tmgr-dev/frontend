import { reactive } from 'vue';
import * as realApi from './api';
import type { RemotePolicyInfo, RemotePolicyLeader } from './types';

export interface RemotePolicyApi {
	policy: () => Promise<RemotePolicyInfo | null>;
	set: (leader: string, ask: boolean) => Promise<RemotePolicyInfo>;
}

export const createRemotePolicyStore = (api: RemotePolicyApi) => {
	const store = reactive({
		leaders: [] as RemotePolicyLeader[],
		busy: false,
		error: '',

		async refresh() {
			try {
				store.leaders = (await api.policy())?.leaders ?? [];
			} catch {
				store.leaders = [];
			}
		},

		async setAsk(leader: string, ask: boolean) {
			if (store.busy) return;
			store.busy = true;
			store.error = '';
			try {
				store.leaders = (await api.set(leader, ask)).leaders;
			} catch (e) {
				store.error = e instanceof Error ? e.message : String(e);
				await store.refresh();
			} finally {
				store.busy = false;
			}
		},
	});
	return store;
};

export type RemotePolicyStore = ReturnType<typeof createRemotePolicyStore>;

let singleton: RemotePolicyStore | null = null;

export const useRemotePolicyStore = () => {
	singleton ??= createRemotePolicyStore({
		policy: realApi.managerRemotePolicy,
		set: realApi.managerRemotePolicySet,
	});
	return singleton;
};
