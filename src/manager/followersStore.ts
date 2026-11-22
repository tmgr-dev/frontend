import { reactive } from 'vue';
import * as realApi from './api';
import type { FollowerInfo, FollowerResult, ManagerMachine } from './types';

export interface FollowersApi {
	followers: () => Promise<FollowerInfo[]>;
	connect: () => Promise<FollowerResult[] | null>;
}

const key = (name: string) => name.trim().toLowerCase();

export const createFollowersStore = (
	api: FollowersApi,
	onDone: () => Promise<unknown> | void,
) => {
	const store = reactive({
		followers: [] as FollowerInfo[],
		results: [] as FollowerResult[],
		connected: [] as string[],
		busy: false,

		get scopeHints(): string[] {
			return store.followers
				.filter((f) => f.chat_enroll === false)
				.map((f) => f.follower);
		},

		pendingCount(machines: ManagerMachine[]): number {
			const known = new Set(machines.map((m) => key(m.label ?? '')));
			return store.followers.filter(
				(f) =>
					!known.has(key(f.follower)) && !store.connected.includes(f.follower),
			).length;
		},

		async refresh() {
			try {
				store.followers = await api.followers();
			} catch {
				store.followers = [];
			}
		},

		async connect() {
			if (store.busy) return;
			store.busy = true;
			try {
				const results = (await api.connect()) ?? [];
				store.results = results;
				store.connected.push(
					...results
						.filter((r) => r.state === 'enrolled' || r.state === 'already')
						.map((r) => r.follower),
				);
				await onDone();
				await store.refresh();
			} catch (e) {
				store.results = [
					{
						follower: '',
						state: 'error',
						message: e instanceof Error ? e.message : String(e),
					},
				];
			} finally {
				store.busy = false;
			}
		},
	});
	return store;
};

export type FollowersStore = ReturnType<typeof createFollowersStore>;

let singleton: FollowersStore | null = null;

export const useFollowersStore = (onDone: () => Promise<unknown> | void) => {
	singleton ??= createFollowersStore(
		{
			followers: realApi.managerFollowers,
			connect: realApi.managerConnectFollowers,
		},
		onDone,
	);
	return singleton;
};
