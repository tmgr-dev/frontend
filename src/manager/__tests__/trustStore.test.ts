import { pairErrorKey } from '../components/trust/trustHelpers';
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
		confirmDeviceKind: jest.fn(async () => {}),
		resetIdentity: jest.fn(async () => {}),
		openJoinCode: jest.fn(async () => ({
			code: 'tmgr-chat-join:v2:abc',
			qr: 'data:image/svg+xml;base64,AAAA',
			expires_at_ms: Date.now() + 600_000,
		})),
		closeJoinCode: jest.fn(async () => {}),
		joinWithCode: jest.fn(async () => {}),
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

	it('confirms a device kind through the api and closes the dialog', async () => {
		const { api, store } = setup(base());
		store.askConfirmKind('d1', 'mobile');
		expect(store.confirmKindTarget).toEqual({ id: 'd1', kind: 'mobile' });
		await store.submitConfirmKind();
		expect(api.confirmDeviceKind).toHaveBeenCalledWith('d1', 'mobile');
		expect(store.confirmKindTarget).toBeNull();
	});

	it('keeps the confirm dialog open and shows a refusal as is', async () => {
		const refusal = 'update these devices first: Phone, Old Mac';
		const { store } = setup(base({ pending_joins: [join('d1')] }), {
			joinDecide: jest.fn(async () => {
				throw refusal;
			}),
			confirmDeviceKind: jest.fn(async () => {
				throw 'this device kind cannot be confirmed';
			}),
		});
		await store.refresh();
		await store.decideJoin(true);
		expect(store.error).toBe(refusal);
		expect(store.current).toMatchObject({ type: 'join' });
		store.askConfirmKind('d1', 'desktop');
		await store.submitConfirmKind();
		expect(store.error).toBe('this device kind cannot be confirmed');
		expect(store.confirmKindTarget).not.toBeNull();
	});

	describe('pairing', () => {
		afterEach(() => {
			jest.useRealTimers();
		});

		it('shows a code with a countdown and wipes it on close', async () => {
			jest.useFakeTimers();
			const { api, store } = setup(base());
			await store.openAddDevice();
			expect(api.openJoinCode).toHaveBeenCalledTimes(1);
			expect(store.pair).toMatchObject({
				mode: 'show',
				code: 'tmgr-chat-join:v2:abc',
				secondsLeft: 600,
			});
			jest.advanceTimersByTime(5000);
			expect(store.pair.secondsLeft).toBe(595);
			const exp = store.pair.expiresAtMs;
			store.closePair();
			expect(store.pair).toMatchObject({ mode: null, code: '', qr: '' });
			expect(api.closeJoinCode).toHaveBeenCalledWith(exp);
			expect(jest.getTimerCount()).toBe(0);
		});

		it('wipes the code at expiry and says it expired', async () => {
			jest.useFakeTimers();
			const { store } = setup(base());
			await store.openAddDevice();
			jest.advanceTimersByTime(600_000);
			expect(store.pair).toMatchObject({
				mode: 'show',
				code: '',
				qr: '',
				expired: true,
			});
			expect(jest.getTimerCount()).toBe(0);
		});

		it('drops a code that arrives after the dialog was closed', async () => {
			let release: (v: {
				code: string;
				qr: string;
				expires_at_ms: number;
			}) => void = () => {};
			const { api, store } = setup(base(), {
				openJoinCode: jest.fn(
					() =>
						new Promise((r) => {
							release = r;
						}),
				),
			});
			const opening = store.openAddDevice();
			store.closePair();
			release({ code: 'late', qr: 'late', expires_at_ms: Date.now() + 60_000 });
			await opening;
			expect(store.pair.code).toBe('');
			expect(store.pair.mode).toBeNull();
			expect(api.closeJoinCode).toHaveBeenCalledTimes(1);
		});

		it('keeps the failure and shows no code when generating fails', async () => {
			const { store } = setup(base(), {
				openJoinCode: jest.fn(async () => {
					throw new Error('chat sync is not set up');
				}),
			});
			await store.openAddDevice();
			expect(store.pair).toMatchObject({
				mode: 'show',
				code: '',
				loading: false,
			});
			expect(store.error).toBe('chat sync is not set up');
		});

		it('never ends blank when the store is busy', async () => {
			const { api, store } = setup(base());
			store.busy = true;
			await store.openAddDevice();
			expect(api.openJoinCode).not.toHaveBeenCalled();
			expect(store.pair.loading).toBe(false);
			expect(store.error).not.toBe('');
		});

		it('never ends blank when no view comes back', async () => {
			const { store } = setup(base(), {
				openJoinCode: jest.fn(async () => undefined as never),
			});
			await store.openAddDevice();
			expect(store.pair.loading).toBe(false);
			expect(store.error).not.toBe('');
		});

		it('is loading while the code is being generated', async () => {
			let release: (v: {
				code: string;
				qr: string;
				expires_at_ms: number;
			}) => void = () => {};
			const { store } = setup(base(), {
				openJoinCode: jest.fn(
					() =>
						new Promise((r) => {
							release = r;
						}),
				),
			});
			jest.useFakeTimers();
			const opening = store.openAddDevice();
			expect(store.pair.loading).toBe(true);
			release({ code: 'c', qr: 'q', expires_at_ms: Date.now() + 60_000 });
			await opening;
			expect(store.pair.loading).toBe(false);
			expect(store.pair.code).toBe('c');
		});

		it('stops waiting after 15 seconds with a timeout error and burns a late code', async () => {
			jest.useFakeTimers();
			let release: (v: {
				code: string;
				qr: string;
				expires_at_ms: number;
			}) => void = () => {};
			const { api, store } = setup(base(), {
				openJoinCode: jest.fn(
					() =>
						new Promise((r) => {
							release = r;
						}),
				),
			});
			const opening = store.openAddDevice();
			await jest.advanceTimersByTimeAsync(14_999);
			expect(store.pair.loading).toBe(true);
			await jest.advanceTimersByTimeAsync(1);
			await opening;
			expect(store.pair).toMatchObject({
				mode: 'show',
				loading: false,
				code: '',
			});
			expect(store.busy).toBe(false);
			expect(pairErrorKey(store.error)).toBe('manager.trust.pair.errorTimeout');
			const exp = Date.now() + 60_000;
			release({ code: 'late', qr: 'late', expires_at_ms: exp });
			await Promise.resolve();
			await Promise.resolve();
			expect(store.pair.code).toBe('');
			expect(api.closeJoinCode).toHaveBeenCalledWith(exp);
		});

		it('maps raw pairing errors to i18n keys', () => {
			expect(pairErrorKey('chat sync is not set up')).toBe(
				'manager.trust.pair.errorNotSetUp',
			);
			expect(
				pairErrorKey('this device is waiting for another device to approve it'),
			).toBe('manager.trust.pair.errorNotApproved');
			expect(pairErrorKey('HTTP 502')).toBe('manager.trust.pair.errorGeneric');
			expect(pairErrorKey('creating the pairing code timed out')).toBe(
				'manager.trust.pair.errorTimeout',
			);
		});

		it('closes and wipes the code once a join request arrives', async () => {
			jest.useFakeTimers();
			const { set, store } = setup(base());
			await store.openAddDevice();
			set(base({ pending_joins: [join('d1')] }));
			await store.refresh();
			expect(store.pair.mode).toBeNull();
			expect(store.pair.code).toBe('');
		});

		it('wipes the code when the panel closes', async () => {
			jest.useFakeTimers();
			const { store } = setup(base());
			store.openPanel();
			await store.openAddDevice();
			store.closePanel();
			expect(store.pair.code).toBe('');
		});

		it('joins with the pasted code, trimmed, then wipes it', async () => {
			const { api, store } = setup(base());
			store.openEnterCode();
			store.pair.text = '  tmgr-chat-join:\nv2:xyz \n';
			await store.submitCode();
			expect(api.joinWithCode).toHaveBeenCalledWith('tmgr-chat-join:v2:xyz');
			expect(store.pair).toMatchObject({ mode: null, text: '' });
		});

		it('keeps the field and shows the error when the join is refused', async () => {
			const { store } = setup(base(), {
				joinWithCode: jest.fn(async () => {
					throw 'the join code is not valid';
				}),
			});
			store.openEnterCode();
			store.pair.text = 'nope';
			await store.submitCode();
			expect(store.error).toBe('the join code is not valid');
			expect(store.pair.mode).toBe('enter');
		});

		it('does not burn anything when closing the enter dialog', () => {
			const { api, store } = setup(base());
			store.openEnterCode();
			store.closePair();
			expect(api.closeJoinCode).not.toHaveBeenCalled();
		});

		it('does not call the api for an empty field', async () => {
			const { api, store } = setup(base());
			store.openEnterCode();
			store.pair.text = '   ';
			await store.submitCode();
			expect(api.joinWithCode).not.toHaveBeenCalled();
		});
	});
});
