jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { put: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: { state: { token: null as any } },
}));

import $axios from '@/plugins/axios';
import store from '@/store';
import { updateUser } from '../user';

const base = { id: 1, email: 'a@b.c', role: 1, name: 'A', settings: [] };
const storage: Record<string, string> = {};

beforeEach(() => {
	jest.clearAllMocks();
	Object.keys(storage).forEach((k) => delete storage[k]);
	(global as any).localStorage = {
		getItem: (k: string) => storage[k] ?? null,
	};
	(store.state as any).token = null;
	($axios.put as jest.Mock).mockResolvedValue({ data: { data: {} } });
});

const sent = () => ($axios.put as jest.Mock).mock.calls[0][1];

test('a name-only save sends neither session field', async () => {
	storage.token = JSON.stringify({ token: 't', refresh_token: 'r1' });
	await updateUser({ ...base, password: null });
	expect(sent()).not.toHaveProperty('logout_other_sessions');
	expect(sent()).not.toHaveProperty('current_refresh_token');
});

test('a password change defaults to signing out other sessions and sends the stored refresh token', async () => {
	storage.token = JSON.stringify({ token: 't', refresh_token: 'r1' });
	await updateUser({ ...base, password: 'secret-1' });
	expect(sent()).toMatchObject({
		logout_other_sessions: true,
		current_refresh_token: 'r1',
	});
});

test('the flag follows the option', async () => {
	await updateUser({ ...base, password: 'secret-1' }, { logoutOtherSessions: false });
	expect(sent().logout_other_sessions).toBe(false);
});

test('falls back to the store token and omits the refresh token when there is none', async () => {
	(store.state as any).token = { token: 't', refresh_token: 'r2' };
	await updateUser({ ...base, password: 'secret-1' });
	expect(sent().current_refresh_token).toBe('r2');

	jest.clearAllMocks();
	(store.state as any).token = { token: 't' };
	await updateUser({ ...base, password: 'secret-1' });
	expect(sent()).not.toHaveProperty('current_refresh_token');
});
