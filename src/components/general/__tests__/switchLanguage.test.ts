import { readFileSync } from 'fs';
import { join } from 'path';

const mockSetLocale = jest.fn();
const mockSaveUserLanguage = jest.fn();

jest.mock('@/i18n', () => ({
	setLocale: (...args: unknown[]) => mockSetLocale(...args),
}));
jest.mock('@/actions/tmgr/user', () => ({
	saveUserLanguage: (...args: unknown[]) => mockSaveUserLanguage(...args),
}));

import { switchLanguage } from '../switchLanguage';

const read = (path: string) =>
	readFileSync(join(__dirname, '..', '..', '..', path), 'utf8');

beforeEach(() => {
	mockSetLocale.mockReset().mockImplementation(async (value: string) => value);
	mockSaveUserLanguage.mockReset().mockResolvedValue(undefined);
});

describe('switchLanguage', () => {
	it('switches the locale and saves it to the account', async () => {
		await expect(switchLanguage('ru', 'en')).resolves.toBe(true);
		expect(mockSetLocale).toHaveBeenCalledWith('ru');
		expect(mockSaveUserLanguage).toHaveBeenCalledWith('ru');
	});

	it('does nothing when the locale is already current', async () => {
		await expect(switchLanguage('en', 'en')).resolves.toBe(true);
		expect(mockSetLocale).not.toHaveBeenCalled();
		expect(mockSaveUserLanguage).not.toHaveBeenCalled();
	});

	it('rolls back to the previous locale when saving fails', async () => {
		mockSaveUserLanguage.mockRejectedValue(new Error('boom'));
		await expect(switchLanguage('de', 'en')).resolves.toBe(false);
		expect(mockSetLocale).toHaveBeenLastCalledWith('en');
	});

	it('does not save when the new catalog fails to load', async () => {
		mockSetLocale.mockImplementation(async (value: string) => {
			if (value === 'ja') throw new Error('catalog');
			return value;
		});
		await expect(switchLanguage('ja', 'en')).resolves.toBe(false);
		expect(mockSaveUserLanguage).not.toHaveBeenCalled();
	});

	it('still reports failure when the rollback also fails', async () => {
		mockSetLocale.mockRejectedValue(new Error('catalog'));
		await expect(switchLanguage('ja', 'en')).resolves.toBe(false);
	});
});

describe('language switcher placement', () => {
	it('sits in the sidebar user menu right after the theme toggle', () => {
		const sidebar = read('components/general/CustomSidebar.vue');
		expect(sidebar).toMatch(
			/<DarkMode \/>\s*<\/DropdownMenuItem>\s*<LanguageMenuItem \/>/,
		);
	});

	it('lists every supported locale and marks the current one', () => {
		const item = read('components/general/LanguageMenuItem.vue');
		expect(item).toContain('v-for="locale in SUPPORTED_LOCALES"');
		expect(item).toContain(':model-value="current"');
		expect(item).toContain('DropdownMenuRadioItem');
	});

	it('is gone from Settings', () => {
		const settings = read('pages/Settings.vue');
		expect(settings).not.toContain('LanguageSetting');
	});

	it('no longer ships the Settings language component', () => {
		expect(() => read('components/general/LanguageSetting.vue')).toThrow();
	});
});
