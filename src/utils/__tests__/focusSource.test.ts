import { FOCUS_SOURCE_ATTR, installFocusSource } from '@/utils/focusSource';

const setup = () => {
	const attrs = new Map<string, string>();
	const target = new EventTarget();
	const doc = {
		addEventListener: target.addEventListener.bind(target),
		documentElement: {
			setAttribute: (k: string, v: string) => attrs.set(k, v),
			removeAttribute: (k: string) => attrs.delete(k),
		},
	} as unknown as Document;
	installFocusSource(doc);
	const key = (init: Partial<KeyboardEvent>) =>
		target.dispatchEvent(Object.assign(new Event('keydown'), init));
	const pointer = () => target.dispatchEvent(new Event('pointerdown'));
	return { attrs, key, pointer };
};

describe('installFocusSource', () => {
	it('marks pointer interaction and clears it on a navigation key', () => {
		const { attrs, key, pointer } = setup();
		expect(attrs.has(FOCUS_SOURCE_ATTR)).toBe(false);

		pointer();
		expect(attrs.get(FOCUS_SOURCE_ATTR)).toBe('pointer');

		key({ key: 'Tab' });
		expect(attrs.has(FOCUS_SOURCE_ATTR)).toBe(false);
	});

	it('keeps pointer mode on modifier-only keys and Meta/Ctrl/Alt shortcuts', () => {
		const { attrs, key, pointer } = setup();
		pointer();
		key({ key: 'Shift', shiftKey: true });
		key({ key: 'Meta', metaKey: true });
		key({ key: 'c', metaKey: true });
		key({ key: 'k', ctrlKey: true });
		key({ key: 'ArrowDown', altKey: true });
		expect(attrs.get(FOCUS_SOURCE_ATTR)).toBe('pointer');
	});

	it('clears pointer mode on Shift+Tab', () => {
		const { attrs, key, pointer } = setup();
		pointer();
		key({ key: 'Tab', shiftKey: true });
		expect(attrs.has(FOCUS_SOURCE_ATTR)).toBe(false);
	});
});
