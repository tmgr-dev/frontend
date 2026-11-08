/**
 * @jest-environment jsdom
 */
import { setLocale, t } from '@/i18n';

describe('i18n under jsdom', () => {
	it('translates and switches locale', async () => {
		expect(t('common.actions.save')).toBe('Save');
		await setLocale('en');
		expect(document.documentElement.lang).toBe('en');
	});
});
