import { avatarInitials } from '@/utils/avatarInitials';

describe('avatarInitials', () => {
	it('takes the first letter of the first two words', () => {
		expect(avatarInitials('Ada Lovelace')).toBe('AL');
		expect(avatarInitials('grace hopper murray')).toBe('GH');
	});

	it('uses one letter when there is one word', () => {
		expect(avatarInitials('Yurij')).toBe('Y');
	});

	it('survives extra spaces and an empty name', () => {
		expect(avatarInitials('  Ada   Lovelace ')).toBe('AL');
		expect(avatarInitials('')).toBe('?');
		expect(avatarInitials(null)).toBe('?');
		expect(avatarInitials('   ')).toBe('?');
	});

	it('keeps a non-latin name readable', () => {
		expect(avatarInitials('Юрий Иванов')).toBe('ЮИ');
	});
});
