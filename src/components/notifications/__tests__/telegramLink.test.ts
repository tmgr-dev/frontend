import { readFileSync } from 'fs';
import { join } from 'path';
import { hasTelegramLink } from '../telegramLink';

describe('hasTelegramLink', () => {
	it('is true with a telegram id or username', () => {
		expect(hasTelegramLink({ telegram_id: 5 })).toBe(true);
		expect(hasTelegramLink({ telegram_username: 'bob' })).toBe(true);
	});

	it('is false for an unlinked, missing or empty user', () => {
		expect(
			hasTelegramLink({ telegram_id: null, telegram_username: null }),
		).toBe(false);
		expect(hasTelegramLink({})).toBe(false);
		expect(hasTelegramLink(null)).toBe(false);
		expect(hasTelegramLink(undefined)).toBe(false);
	});
});

describe('NotificationSettingsForm', () => {
	const source = readFileSync(
		join(__dirname, '..', 'NotificationSettingsForm.vue'),
		'utf8',
	);

	it('reads the user from the store state, not a missing getter', () => {
		expect(source).not.toContain('user/getUser');
		expect(source).toContain('hasTelegramLink(store.state.user)');
	});
});
