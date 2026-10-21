import { isSendEnter } from '../commentComposer';

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
