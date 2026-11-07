import { createPhonesController, type PhonesApi } from '../controller';
import type { PairingView, PhoneView } from '../types';

const phone = (id: string): PhoneView => ({
	id,
	name: `Phone ${id}`,
	fingerprint: 'abcdef0123',
	paired_at_ms: 1,
	last_used_ms: 2,
	host: 'https://api.example.com/api/',
});

const confirming: PairingView = {
	state: 'confirm',
	qr_svg_data_url: 'data:image/svg+xml;base64,AAAA',
	expires_ms: 9,
	attempts_left: 3,
};

const makeApi = (over: Partial<PhonesApi> = {}) => {
	const api = {
		list: jest.fn().mockResolvedValue([phone('a'), phone('b')]),
		pairing: jest.fn().mockResolvedValue(null),
		confirm: jest.fn().mockResolvedValue(undefined),
		cancel: jest.fn().mockResolvedValue(undefined),
		revoke: jest.fn().mockResolvedValue(undefined),
		...over,
	};
	return api;
};

describe('phones controller', () => {
	it('loads phones and the active pairing', async () => {
		const api = makeApi({ pairing: jest.fn().mockResolvedValue(confirming) });
		const c = createPhonesController(api);
		await c.refresh();
		expect(c.state.phones).toHaveLength(2);
		expect(c.state.pairing?.state).toBe('confirm');
	});

	it('revokes only after the confirmation step', async () => {
		const api = makeApi();
		const c = createPhonesController(api);
		await c.refresh();
		c.askRevoke('a');
		expect(api.revoke).not.toHaveBeenCalled();
		c.cancelRevoke();
		await c.confirmRevoke();
		expect(api.revoke).not.toHaveBeenCalled();
		c.askRevoke('b');
		await c.confirmRevoke();
		expect(api.revoke).toHaveBeenCalledWith('b');
		expect(c.state.revokeId).toBeNull();
		expect(api.list).toHaveBeenCalledTimes(2);
	});

	it('sends the code only when it is six digits and a confirm is pending', async () => {
		const api = makeApi({ pairing: jest.fn().mockResolvedValue(confirming) });
		const c = createPhonesController(api);
		await c.refresh();
		await c.submitCode('12345');
		await c.submitCode('12345x');
		expect(api.confirm).not.toHaveBeenCalled();
		await c.submitCode('123456');
		expect(api.confirm).toHaveBeenCalledWith('123456');
	});

	it('does not send a code while waiting for the phone', async () => {
		const api = makeApi({
			pairing: jest.fn().mockResolvedValue({ ...confirming, state: 'waiting' }),
		});
		const c = createPhonesController(api);
		await c.refresh();
		await c.submitCode('123456');
		expect(api.confirm).not.toHaveBeenCalled();
	});

	it('shows a command error and refreshes afterwards', async () => {
		const api = makeApi({
			pairing: jest.fn().mockResolvedValue(confirming),
			confirm: jest.fn().mockRejectedValue('Wrong code'),
		});
		const c = createPhonesController(api);
		await c.refresh();
		await c.submitCode('000000');
		expect(c.state.error).toBe('Wrong code');
		expect(api.pairing).toHaveBeenCalledTimes(2);
		expect(c.state.busy).toBe(false);
	});

	it('cancels the pairing', async () => {
		const api = makeApi();
		const c = createPhonesController(api);
		await c.cancelPairing();
		expect(api.cancel).toHaveBeenCalled();
	});
});
