import { currentLocale, setLocale } from '@/i18n';

test('a slower earlier request does not override a newer choice', async () => {
	const slow = setLocale('ru');
	const fast = setLocale('en');
	await Promise.all([slow, fast]);
	expect(currentLocale()).toBe('en');
	await expect(slow).resolves.toBe('en');
});
