import { createPairingStore, type PairingApi } from '../pairingStore';
import type { ManagerMachine, PairResult } from '../types';

const machines: ManagerMachine[] = [
	{ machine: 'mx_a', label: 'Studio', role: 'top', local: true },
];

const setup = (result: PairResult | null | Error, list = machines) => {
	const api: PairingApi = {
		pair: jest.fn(async () => {
			if (result instanceof Error) throw result;
			return result;
		}),
		machines: jest.fn(async () => list),
	};
	return { api, store: createPairingStore(api) };
};

describe('pairing store', () => {
	it('paired shows a notice with the machine label and device id', async () => {
		const { store } = setup({
			state: 'paired',
			machine: 'mx_a',
			device_id: 'dev1',
		});
		await store.start();
		expect(store.notice).toEqual({ deviceId: 'dev1', label: 'Studio' });
		expect(store.machines).toEqual(machines);
	});

	it('paired falls back to the machine id, then to a generic label', async () => {
		const a = setup({ state: 'paired', machine: 'mx_zz', device_id: 'd' });
		await a.store.start();
		expect(a.store.notice?.label).toBe('mx_zz');
		const b = setup({ state: 'paired', device_id: 'd' });
		await b.store.start();
		expect(b.store.notice?.label).toBe('this computer');
	});

	it('already and pending stay quiet', async () => {
		for (const state of ['already', 'pending'] as const) {
			const { store } = setup({ state });
			await store.start();
			expect(store.notice).toBeNull();
			expect(store.blockers).toEqual([]);
		}
	});

	it('blocked exposes the blocking device labels', async () => {
		const { store } = setup({ state: 'blocked', blockers: ['iPhone', 'Mac'] });
		await store.start();
		expect(store.blockers).toEqual(['iPhone', 'Mac']);
	});

	it('quiet states only warn', async () => {
		const warn = jest.spyOn(console, 'warn').mockImplementation(() => {});
		for (const state of ['no_cli', 'no_runner', 'error'] as const) {
			const { store } = setup({ state, message: 'boom' });
			await store.start();
			expect(store.notice).toBeNull();
			expect(store.blockers).toEqual([]);
		}
		expect(warn).toHaveBeenCalledTimes(3);
		warn.mockRestore();
	});

	it('a missing command (null result) and a thrown error are no-ops', async () => {
		const warn = jest.spyOn(console, 'warn').mockImplementation(() => {});
		const a = setup(null);
		await a.store.start();
		const b = setup(new Error('boom'));
		await b.store.start();
		expect(a.store.notice).toBeNull();
		expect(b.store.notice).toBeNull();
		expect(b.store.machines).toEqual(machines);
		warn.mockRestore();
	});

	it('pairs once per start and dismiss clears the notice', async () => {
		const { api, store } = setup({
			state: 'paired',
			machine: 'mx_a',
			device_id: 'd',
		});
		await store.start();
		store.dismiss();
		expect(store.notice).toBeNull();
		await store.start();
		expect(api.pair).toHaveBeenCalledTimes(2);
		await store.refreshMachines();
		expect(api.pair).toHaveBeenCalledTimes(2);
	});

	it('refreshMachines keeps the old list when the call fails', async () => {
		const { api, store } = setup({ state: 'already' });
		await store.start();
		(api.machines as jest.Mock).mockRejectedValueOnce(new Error('x'));
		await store.refreshMachines();
		expect(store.machines).toEqual(machines);
	});
});
