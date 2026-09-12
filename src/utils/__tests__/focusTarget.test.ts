import { focusField, resolveFocusTarget } from '../focusTarget';

const field = (tagName: string) => ({ tagName, focus: jest.fn() });
const wrapper = (root: unknown) => ({ $el: root });
const container = (found: unknown) => ({ tagName: 'DIV', querySelector: jest.fn(() => found) });

describe('resolveFocusTarget', () => {
	it('takes a plain input element as it is', () => {
		const input = field('INPUT');

		expect(resolveFocusTarget(input)).toBe(input);
	});

	it('reaches through a component wrapper to its field', () => {
		const textarea = field('TEXTAREA');

		expect(resolveFocusTarget(wrapper(container(textarea)))).toBe(textarea);
	});

	it('uses the wrapper root when it is the field itself', () => {
		const input = field('INPUT');

		expect(resolveFocusTarget(wrapper(input))).toBe(input);
	});

	it('asks for a textarea or an input, nothing else', () => {
		const root = container(field('INPUT'));

		resolveFocusTarget(wrapper(root));

		expect(root.querySelector).toHaveBeenCalledWith('textarea, input');
	});

	it('gives up quietly when there is no field', () => {
		expect(resolveFocusTarget(null)).toBeNull();
		expect(resolveFocusTarget(undefined)).toBeNull();
		expect(resolveFocusTarget({})).toBeNull();
		expect(resolveFocusTarget(wrapper(container(null)))).toBeNull();
	});
});

describe('focusField', () => {
	it('focuses the field it found', () => {
		const input = field('INPUT');

		expect(focusField(input)).toBe(true);
		expect(input.focus).toHaveBeenCalledTimes(1);
	});

	it('does nothing when there is no field to focus', () => {
		expect(focusField(null)).toBe(false);
	});
});
