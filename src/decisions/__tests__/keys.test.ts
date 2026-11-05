import { isSubmitKey, mapKey, type KeyContext } from '../keys';

const ctx = (over: Partial<KeyContext> = {}): KeyContext => ({
	typing: false,
	nativeEnter: false,
	optionCount: 3,
	hasRecommended: true,
	canAnswer: true,
	canDismiss: true,
	...over,
});

describe('mapKey', () => {
	it('Enter accepts the recommended option, Shift+Enter also opens the note', () => {
		expect(mapKey({ key: 'Enter' }, ctx())).toEqual({
			type: 'accept',
			withNote: false,
		});
		expect(mapKey({ key: 'Enter', shiftKey: true }, ctx())).toEqual({
			type: 'accept',
			withNote: true,
		});
		expect(mapKey({ key: 'Enter' }, ctx({ hasRecommended: false }))).toBeNull();
	});

	it('Enter on a focused link or button never answers, with or without Shift', () => {
		const native = ctx({ nativeEnter: true });
		expect(mapKey({ key: 'Enter' }, native)).toBeNull();
		expect(mapKey({ key: 'Enter', shiftKey: true }, native)).toBeNull();
		expect(mapKey({ key: '2', code: 'Digit2' }, native)).toEqual({
			type: 'pick',
			index: 1,
			withNote: false,
		});
	});

	it('digits pick an option within range; Shift opens the note', () => {
		expect(mapKey({ key: '2', code: 'Digit2' }, ctx())).toEqual({
			type: 'pick',
			index: 1,
			withNote: false,
		});
		expect(mapKey({ key: '@', code: 'Digit2', shiftKey: true }, ctx())).toEqual(
			{ type: 'pick', index: 1, withNote: true },
		);
		expect(mapKey({ key: '4', code: 'Digit4' }, ctx())).toBeNull();
		expect(mapKey({ key: '0', code: 'Digit0' }, ctx())).toBeNull();
	});

	it('maps letters case-insensitively', () => {
		expect(mapKey({ key: 't' }, ctx())).toEqual({ type: 'free' });
		expect(mapKey({ key: 'D' }, ctx())).toEqual({ type: 'defer' });
		expect(mapKey({ key: 'c' }, ctx())).toEqual({ type: 'chat' });
		expect(mapKey({ key: 'x' }, ctx())).toEqual({ type: 'dismiss' });
	});

	it('maps arrows to card navigation', () => {
		expect(mapKey({ key: 'ArrowDown' }, ctx())).toEqual({ type: 'next' });
		expect(mapKey({ key: 'ArrowUp' }, ctx())).toEqual({ type: 'prev' });
	});

	it('ignores everything but Escape while typing', () => {
		const typing = ctx({ typing: true });
		expect(mapKey({ key: 'Escape' }, typing)).toEqual({ type: 'blur' });
		for (const key of ['Enter', 'd', 'x', '1', 'ArrowDown']) {
			expect(
				mapKey({ key, code: key === '1' ? 'Digit1' : undefined }, typing),
			).toBeNull();
		}
	});

	it('ignores modified keys and answer keys when answering is not possible', () => {
		expect(mapKey({ key: 'd', metaKey: true }, ctx())).toBeNull();
		const locked = ctx({ canAnswer: false, canDismiss: false });
		for (const key of ['Enter', 't', 'd', 'x'])
			expect(mapKey({ key }, locked)).toBeNull();
		expect(mapKey({ key: '1', code: 'Digit1' }, locked)).toBeNull();
		expect(mapKey({ key: 'c' }, locked)).toEqual({ type: 'chat' });
	});

	it('allows dismiss alone for cards that cannot be answered', () => {
		const hook = ctx({ canAnswer: false, canDismiss: true, optionCount: 0 });
		expect(mapKey({ key: 'x' }, hook)).toEqual({ type: 'dismiss' });
	});
});

describe('isSubmitKey', () => {
	it('submits on Enter and Cmd/Ctrl+Enter but not Shift+Enter', () => {
		expect(isSubmitKey({ key: 'Enter' })).toBe(true);
		expect(isSubmitKey({ key: 'Enter', metaKey: true })).toBe(true);
		expect(isSubmitKey({ key: 'Enter', ctrlKey: true, shiftKey: true })).toBe(
			true,
		);
		expect(isSubmitKey({ key: 'Enter', shiftKey: true })).toBe(false);
		expect(isSubmitKey({ key: 'a' })).toBe(false);
	});
});
