import { readFileSync } from 'fs';
import { join } from 'path';
import { syncSessionFromStorage } from '../sessionSync';
import { tokenSubject } from '../tokenSync';

const fakeJwt = (sub: unknown) =>
	['h', Buffer.from(JSON.stringify({ sub })).toString('base64url'), 'sig'].join(
		'.',
	);
const envelope = (sub: unknown, refresh = 'r') => ({
	token: fakeJwt(sub),
	refresh_token: refresh,
});
const event = (value: unknown) => ({
	key: 'token',
	newValue: value === null ? null : JSON.stringify(value),
});

function setup(initial: { token: any; userId?: number }) {
	const state = { token: initial.token, userId: initial.userId };
	const deps = {
		getToken: () => state.token,
		getUserId: () => state.userId,
		commitToken: jest.fn((token: any) => {
			state.token = token;
			const sub = tokenSubject(token);
			if (sub != null && sub !== state.userId) state.userId = undefined;
		}),
		signOut: jest.fn(),
		clearInFlight: jest.fn(),
		getUser: jest.fn().mockResolvedValue({}),
		disconnectRealtime: jest.fn(),
	};
	return { state, deps };
}

test('ignores events that do not change the token', async () => {
	const current = envelope('2');
	const { deps } = setup({ token: current, userId: 2 });
	await syncSessionFromStorage({ key: 'theme', newValue: 'dark' }, deps);
	await syncSessionFromStorage(event(current), deps);
	expect(deps.commitToken).not.toHaveBeenCalled();
	expect(deps.signOut).not.toHaveBeenCalled();
	expect(deps.getUser).not.toHaveBeenCalled();
});

test('a sign-out in another tab calls the sign-out path without committing the token first', async () => {
	const { state, deps } = setup({ token: envelope('2'), userId: 2 });
	deps.signOut.mockImplementation(() => {
		expect(deps.commitToken).not.toHaveBeenCalled();
		state.token = null;
	});
	await syncSessionFromStorage(event(null), deps);
	expect(deps.signOut).toHaveBeenCalledTimes(1);
	expect(deps.commitToken).not.toHaveBeenCalled();
	expect(deps.getUser).not.toHaveBeenCalled();
});

test('a sign-out path that keeps its token still mirrors the null token', async () => {
	const { deps } = setup({ token: envelope(5), userId: 5 });
	await syncSessionFromStorage(event(null), deps);
	expect(deps.signOut).toHaveBeenCalledTimes(1);
	expect(deps.commitToken).toHaveBeenCalledWith(null);
});

test('a rotation for the same account does not refetch the user', async () => {
	const { deps } = setup({ token: envelope('2', 'r1'), userId: 2 });
	await syncSessionFromStorage(event(envelope('2', 'r2')), deps);
	expect(deps.commitToken).toHaveBeenCalledTimes(1);
	expect(deps.clearInFlight).not.toHaveBeenCalled();
	expect(deps.getUser).not.toHaveBeenCalled();
});

test('a different account clears in-flight requests and refetches the user', async () => {
	const { deps } = setup({ token: envelope('2'), userId: 2 });
	await syncSessionFromStorage(event(envelope('5')), deps);
	expect(deps.commitToken).toHaveBeenCalledTimes(1);
	expect(deps.clearInFlight).toHaveBeenCalledTimes(1);
	expect(deps.getUser).toHaveBeenCalledTimes(1);
	expect(deps.disconnectRealtime).not.toHaveBeenCalled();
});

test('an unreadable subject refetches the user', async () => {
	const { deps } = setup({ token: envelope('2'), userId: 2 });
	await syncSessionFromStorage(
		event({ token: 'opaque', refresh_token: 'r' }),
		deps,
	);
	expect(deps.getUser).toHaveBeenCalledTimes(1);
});

test('a token adopted while no user is loaded refetches the user', async () => {
	const { deps } = setup({ token: null, userId: undefined });
	await syncSessionFromStorage(event(envelope('5')), deps);
	expect(deps.getUser).toHaveBeenCalledTimes(1);
});

test('a failed refetch disconnects realtime while the token is unchanged', async () => {
	const { deps } = setup({ token: envelope('2'), userId: 2 });
	deps.getUser.mockRejectedValue(new Error('offline'));
	await syncSessionFromStorage(event(envelope('5')), deps);
	expect(deps.disconnectRealtime).toHaveBeenCalledTimes(1);
});

test('a failed refetch does not disconnect after a further token change', async () => {
	const { state, deps } = setup({ token: envelope('2'), userId: 2 });
	deps.getUser.mockImplementation(async () => {
		state.token = envelope('6');
		throw new Error('offline');
	});
	await syncSessionFromStorage(event(envelope('5')), deps);
	expect(deps.disconnectRealtime).not.toHaveBeenCalled();
});

describe('main.ts wiring', () => {
	const main = readFileSync(join(__dirname, '../../main.ts'), 'utf8');

	it('routes the storage listener through the session sync', () => {
		expect(main).toContain('syncSessionFromStorage');
		expect(main).not.toMatch(/tokenFromStorageEvent\(event/);
	});

	it('signs out without marking the session expired', () => {
		expect(main).toMatch(/hardLogout\(\{ expired: false \}\)/);
	});
});

test('main.ts reloads workspaces after adopting another account', () => {
	const source = readFileSync(join(__dirname, '..', '..', 'main.ts'), 'utf8');
	expect(source).toContain("store.dispatch('loadWorkspaces')");
});
