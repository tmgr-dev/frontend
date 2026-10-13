import { FOCUS_SOURCE_ATTR, installFocusSource } from '@/utils/focusSource';

describe('installFocusSource', () => {
	it('marks pointer interaction and clears it on any key', () => {
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
		expect(attrs.has(FOCUS_SOURCE_ATTR)).toBe(false);

		target.dispatchEvent(new Event('pointerdown'));
		expect(attrs.get(FOCUS_SOURCE_ATTR)).toBe('pointer');

		target.dispatchEvent(new Event('keydown'));
		expect(attrs.has(FOCUS_SOURCE_ATTR)).toBe(false);
	});
});
