import { createTrustStore, type ManagerTrustApi } from '../trustStore';
import type { TrustState } from '../types';

const base = (over: Partial<TrustState> = {}): TrustState => ({
	registered: true,
	self_fp: 'aa'.repeat(16),
	devices: [],
	pending_joins: [],
	...over,
});

const join = (id: string) => ({
	device_id: id,
	label: `dev ${id}`,
	fp: 'ab12cd34ef56ab12',
	requested_at_ms: 1,
});

const revoke = (id: string) => ({
	statement_id: id,
	revoker_label: 'Phone',
	revoker_fp: 'ab12cd34',
	target_id: 't1',
	target_label: 'Laptop',
	target_fp: 'ee00ff11',
});

const setup = (initial: TrustState, over: Partial<ManagerTrustApi> = {}) => {
	let current = initial;
	let push: () => void = () => {};
	const api: ManagerTrustApi = {
		state: jest.fn(async () => current),
		joinDecide: jest.fn(async () => {}),
		revokeDecide: jest.fn(async () => {}),
		revoke: jest.fn(async () => {}),
		resetIdentity: jest.fn(async () => {}),
		onTrust: jest.fn(async (h) => {
			push = h;
			return () => {};
		}),
		...over,
	};
	return {
		api,
		store: createTrustStore(api),
		set: (s: TrustState) => (current = s),
		emit: () => push(),
	};
};

describe('trust store', () => {
	it('queues joins before revokes', async () => {
		const { store } = setup(
			base({ pending_joins: [join('d1')], pending_revokes: [revoke('r1')] }),
		);
		await store.refresh();
		expect(store.current).toMatchObject({ type: 'join' });
		expect(store.queueLength).toBe(2);
	});

	it('decideJoin calls api once, drops the item and shows the next', async () => {
		const { api, store } = setup(
			base({ pending_joins: [join('d1')], pending_revokes: [revoke('r1')] }),
		);
		await store.refresh();
		await Promise.all([store.decideJoin(true), store.decideJoin(true)]);
		expect(api.joinDecide).toHaveBeenCalledTimes(1);
		expect(api.joinDecide).toHaveBeenCalledWith('d1', true);
		expect(store.current).toMatchObject({ type: 'revoke' });
	});

	it('decideRevoke passes the id and accept flag', async () => {
		const { api, store } = setup(base({ pending_revokes: [revoke('r1')] }));
		await store.refresh();
		await store.decideRevoke(false);
		expect(api.revokeDecide).toHaveBeenCalledWith('r1', false);
		expect(store.current).toBeNull();
	});

	it('keeps the item and reports the error when a decision fails', async () => {
		const { store } = setup(base({ pending_joins: [join('d1')] }), {
			joinDecide: jest.fn(async () => {
				throw new Error('boom');
			}),
		});
		await store.refresh();
		await store.decideJoin(true);
		expect(store.error).toBe('boom');
		expect(store.busy).toBe(false);
		expect(store.current).toMatchObject({ type: 'join' });
	});

	it('revoke opens a confirm and only confirmRevoke calls the api', async () => {
		const { api, store } = setup(base());
		store.revoke('d9');
		expect(store.revokeTarget).toBe('d9');
		expect(api.revoke).not.toHaveBeenCalled();
		await store.confirmRevoke();
		expect(api.revoke).toHaveBeenCalledWith('d9');
		expect(store.revokeTarget).toBeNull();
	});

	it('cancelRevoke closes without calling the api', () => {
		const { api, store } = setup(base());
		store.revoke('d9');
		store.cancelRevoke();
		expect(store.revokeTarget).toBeNull();
		expect(api.revoke).not.toHaveBeenCalled();
	});

	it('resetIdentity needs the exact RESET phrase', async () => {
		const { api, store } = setup(base());
		store.openReset();
		store.resetText = 'reset';
		expect(store.canReset).toBe(false);
		expect(await store.resetIdentity()).toBe(false);
		expect(api.resetIdentity).not.toHaveBeenCalled();
		store.resetText = 'RESET';
		expect(store.canReset).toBe(true);
		expect(await store.resetIdentity()).toBe(true);
		expect(api.resetIdentity).toHaveBeenCalledTimes(1);
		expect(store.resetOpen).toBe(false);
		expect(store.resetText).toBe('');
	});

	it('refreshes on the trust event and stops after dispose', async () => {
		const unlisten = jest.fn();
		let handler: () => void = () => {};
		const { api, store, set } = setup(base(), {
			onTrust: jest.fn(async (h) => {
				handler = h;
				return unlisten;
			}),
		});
		await store.init();
		set(base({ pending_joins: [join('d2')] }));
		handler();
		await new Promise((r) => setTimeout(r, 0));
		expect(api.state).toHaveBeenCalledTimes(2);
		expect(store.current).toMatchObject({ type: 'join' });
		store.dispose();
		expect(unlisten).toHaveBeenCalled();
	});

	it('refresh failure sets error', async () => {
		const { store } = setup(base(), {
			state: jest.fn(async () => {
				throw new Error('down');
			}),
		});
		await store.refresh();
		expect(store.error).toBe('down');
	});
});
