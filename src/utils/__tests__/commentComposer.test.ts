import { autosizeTextarea, isEnterSend, isSendEnter } from '../commentComposer';

const enter = (over: Partial<KeyboardEvent> = {}) => ({
	shiftKey: false,
	isComposing: false,
	keyCode: 13,
	...over,
});

describe('isSendEnter', () => {
	it('sends on plain Enter', () => {
		expect(isSendEnter(enter())).toBe(true);
	});

	it('keeps Shift+Enter as a line break', () => {
		expect(isSendEnter(enter({ shiftKey: true }))).toBe(false);
	});

	it('does not send while an IME composition is active', () => {
		expect(isSendEnter(enter({ isComposing: true }))).toBe(false);
	});

	it('does not send on the Enter that commits an IME composition in WebKit', () => {
		expect(isSendEnter(enter({ keyCode: 229 }))).toBe(false);
	});
});

describe('isEnterSend', () => {
	const key = (init: Partial<KeyboardEvent> = {}) =>
		({
			key: 'Enter',
			shiftKey: false,
			isComposing: false,
			keyCode: 13,
			...init,
		} as KeyboardEvent);

	it('sends on plain Enter only', () => {
		expect(isEnterSend(key())).toBe(true);
		expect(isEnterSend(key({ shiftKey: true }))).toBe(false);
		expect(isEnterSend(key({ key: 'a' }))).toBe(false);
	});

	it('does not send while an IME composition is active', () => {
		expect(isEnterSend(key({ isComposing: true }))).toBe(false);
		expect(isEnterSend(key({ keyCode: 229 }))).toBe(false);
	});
});

describe('autosizeTextarea', () => {
	const el = (scrollHeight: number) =>
		({
			scrollHeight,
			style: {} as Record<string, string>,
		} as unknown as HTMLTextAreaElement);

	it('grows with content up to the max height, then scrolls', () => {
		const small = el(40);
		autosizeTextarea(small, 100);
		expect(small.style.height).toBe('40px');
		expect(small.style.overflowY).toBe('hidden');
		const big = el(300);
		autosizeTextarea(big, 100);
		expect(big.style.height).toBe('100px');
		expect(big.style.overflowY).toBe('auto');
	});

	it('ignores a missing element', () => {
		expect(() => autosizeTextarea(null)).not.toThrow();
	});
});
