import { isSaveHotkey } from '@/utils/saveHotkey';

const event = (overrides: Partial<Parameters<typeof isSaveHotkey>[0]> = {}) => ({
	type: 'keydown',
	code: 'KeyS',
	ctrlKey: true,
	metaKey: false,
	repeat: false,
	...overrides,
});

describe('isSaveHotkey', () => {
	it('matches Ctrl+S on keydown', () => {
		expect(isSaveHotkey(event())).toBe(true);
	});

	it('matches Cmd+S on keydown', () => {
		expect(isSaveHotkey(event({ ctrlKey: false, metaKey: true }))).toBe(true);
	});

	it('ignores the keyup that follows the same shortcut (useMagicKeys fires both)', () => {
		expect(isSaveHotkey(event({ type: 'keyup' }))).toBe(false);
	});

	it('ignores auto-repeat while the keys are held', () => {
		expect(isSaveHotkey(event({ repeat: true }))).toBe(false);
	});

	it('ignores S without a modifier and other keys with a modifier', () => {
		expect(isSaveHotkey(event({ ctrlKey: false }))).toBe(false);
		expect(isSaveHotkey(event({ code: 'KeyA' }))).toBe(false);
	});
});
