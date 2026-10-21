/** @jest-environment jsdom */
jest.mock('@/store', () => ({
	__esModule: true,
	default: { state: { modalStack: ['m'] } },
}));
import { useModalEscHandler } from '../useModalEscHandler';

const esc = () =>
	new KeyboardEvent('keydown', {
		key: 'Escape',
		bubbles: true,
		cancelable: true,
	});

describe('useModalEscHandler with a data-esc-blur field', () => {
	let now = 0;
	beforeEach(() => {
		now += 10_000;
		jest.spyOn(Date, 'now').mockReturnValue(now);
	});
	afterEach(() => {
		jest.restoreAllMocks();
		document.body.innerHTML = '';
	});

	const pressEscIn = (html: string, value: string) => {
		document.body.innerHTML = html;
		const field = document.querySelector('textarea')!;
		field.value = value;
		const close = jest.fn();
		const { registerModal, unregisterModal } = useModalEscHandler();
		registerModal('m', close);
		const event = esc();
		field.dispatchEvent(event);
		unregisterModal('m');
		return { close, event };
	};

	it('leaves a marked field with a draft alone', () => {
		const { close, event } = pressEscIn(
			'<textarea data-esc-blur></textarea>',
			'draft',
		);
		expect(close).not.toHaveBeenCalled();
		expect(event.defaultPrevented).toBe(false);
	});

	it('closes the modal from an empty marked field', () => {
		const { close } = pressEscIn('<textarea data-esc-blur></textarea>', '');
		expect(close).toHaveBeenCalledTimes(1);
	});

	it('closes the modal from an unmarked field with text', () => {
		const { close } = pressEscIn('<textarea></textarea>', 'draft');
		expect(close).toHaveBeenCalledTimes(1);
	});
});
