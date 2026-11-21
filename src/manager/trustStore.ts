import { reactive } from 'vue';
import * as realApi from './api';
import {
	RESET_PHRASE,
	type PendingJoin,
	type PendingRevoke,
	type TrustState,
} from './types';

export interface ManagerTrustApi {
	state: () => Promise<TrustState>;
	joinDecide: (deviceId: string, accept: boolean) => Promise<void>;
	revokeDecide: (id: string, accept: boolean) => Promise<void>;
	revoke: (deviceId: string) => Promise<void>;
	confirmDeviceKind: (deviceId: string, kind: ConfirmableKind) => Promise<void>;
	resetIdentity: () => Promise<void>;
	onTrust: typeof realApi.onManagerTrust;
}

export type ConfirmableKind = 'desktop' | 'mobile';

export type TrustItem =
	| { type: 'join'; join: PendingJoin }
	| { type: 'revoke'; revoke: PendingRevoke };

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const createTrustStore = (api: ManagerTrustApi) => {
	let unlisten: (() => void) | null = null;

	const store = reactive({
		trust: null as TrustState | null,
		busy: false,
		error: '',
		panelOpen: false,
		revokeTarget: null as string | null,
		confirmKindTarget: null as { id: string; kind: ConfirmableKind } | null,
		resetOpen: false,
		resetText: '',
		deferred: [] as string[],

		get queue(): TrustItem[] {
			const t = store.trust;
			if (!t) return [];
			return [
				...t.pending_joins.map((join) => ({ type: 'join' as const, join })),
				...(t.pending_revokes ?? [])
					.filter((r) => !store.deferred.includes(r.statement_id))
					.map((revoke) => ({
						type: 'revoke' as const,
						revoke,
					})),
			];
		},
		get current(): TrustItem | null {
			return store.queue[0] ?? null;
		},
		get queueLength(): number {
			return store.queue.length;
		},
		get canReset(): boolean {
			return store.resetText === RESET_PHRASE;
		},

		async refresh() {
			try {
				store.trust = await api.state();
			} catch (e) {
				store.error = errorText(e);
			}
		},

		async init() {
			await store.refresh();
			try {
				unlisten = await api.onTrust(() => void store.refresh());
			} catch (e) {
				store.error = errorText(e);
			}
		},

		dispose() {
			unlisten?.();
			unlisten = null;
		},

		async run(action: () => Promise<void>): Promise<boolean> {
			if (store.busy) return false;
			store.busy = true;
			store.error = '';
			try {
				await action();
				return true;
			} catch (e) {
				store.error = errorText(e);
				return false;
			} finally {
				store.busy = false;
			}
		},

		async decideJoin(accept: boolean) {
			const item = store.current;
			if (item?.type !== 'join') return;
			const id = item.join.device_id;
			const ok = await store.run(() => api.joinDecide(id, accept));
			if (!ok || !store.trust) return;
			store.trust.pending_joins = store.trust.pending_joins.filter(
				(j) => j.device_id !== id,
			);
			await store.refresh();
		},

		async decideRevoke(accept: boolean) {
			const item = store.current;
			if (item?.type !== 'revoke') return;
			const id = item.revoke.statement_id;
			const ok = await store.run(() => api.revokeDecide(id, accept));
			if (!ok || !store.trust) return;
			store.trust.pending_revokes = (store.trust.pending_revokes ?? []).filter(
				(r) => r.statement_id !== id,
			);
			await store.refresh();
		},

		deferRevoke() {
			const item = store.current;
			if (item?.type === 'revoke')
				store.deferred.push(item.revoke.statement_id);
		},

		openPanel() {
			store.panelOpen = true;
		},
		closePanel() {
			store.panelOpen = false;
		},

		revoke(deviceId: string) {
			store.revokeTarget = deviceId;
		},
		cancelRevoke() {
			store.revokeTarget = null;
		},
		async confirmRevoke() {
			const id = store.revokeTarget;
			if (!id) return;
			if (await store.run(() => api.revoke(id))) {
				store.revokeTarget = null;
				await store.refresh();
			}
		},

		askConfirmKind(id: string, kind: ConfirmableKind) {
			store.confirmKindTarget = { id, kind };
		},
		cancelConfirmKind() {
			store.confirmKindTarget = null;
		},
		async submitConfirmKind() {
			const target = store.confirmKindTarget;
			if (!target) return;
			if (
				await store.run(() => api.confirmDeviceKind(target.id, target.kind))
			) {
				store.confirmKindTarget = null;
				await store.refresh();
			}
		},

		openReset() {
			store.resetText = '';
			store.resetOpen = true;
		},
		cancelReset() {
			store.resetOpen = false;
			store.resetText = '';
		},
		async resetIdentity(): Promise<boolean> {
			if (!store.canReset) return false;
			const ok = await store.run(() => api.resetIdentity());
			if (!ok) return false;
			store.cancelReset();
			await store.refresh();
			return true;
		},
	});

	return store;
};

export type TrustStore = ReturnType<typeof createTrustStore>;

let singleton: TrustStore | null = null;

export const useTrustStore = (): TrustStore => {
	singleton ??= createTrustStore({
		state: realApi.managerTrustState,
		joinDecide: realApi.managerJoinDecide,
		revokeDecide: realApi.managerRevokeDecide,
		revoke: realApi.managerRevoke,
		confirmDeviceKind: realApi.managerConfirmDeviceKind,
		resetIdentity: realApi.managerResetIdentity,
		onTrust: realApi.onManagerTrust,
	});
	return singleton;
};
