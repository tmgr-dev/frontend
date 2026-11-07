import { linkLocalProfile, unlinkLocalProfile } from '@/utils/localProfile';

const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => invoke(...args),
}));

beforeEach(() => invoke.mockReset());

test('link sends the account id, api base and token', async () => {
	invoke.mockResolvedValue({ linked_user_id: 7 });
	await linkLocalProfile(7, 'https://api.example/api/', 'tok');
	expect(invoke).toHaveBeenCalledWith('local_profile_link', {
		userId: 7,
		apiBase: 'https://api.example/api/',
		token: 'tok',
	});
});

test('unlink calls the command without arguments and returns its answer', async () => {
	invoke.mockResolvedValue(true);
	await expect(unlinkLocalProfile()).resolves.toBe(true);
	expect(invoke).toHaveBeenCalledWith('local_profile_unlink', undefined);
});
