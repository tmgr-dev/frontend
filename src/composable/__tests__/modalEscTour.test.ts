/** @jest-environment jsdom */
jest.mock('@/store', () => ({ __esModule: true, default: { state: { modalStack: ['m'] } } }));
import { useModalEscHandler } from '../useModalEscHandler';

const esc = () => new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });

describe('useModalEscHandler with a tour popover open', () => {
	afterEach(() => {
		document.body.innerHTML = '';
	});

	it('still closes the modal while a tour popover is the only open dialog', () => {
		document.body.innerHTML = '<div role="dialog" data-state="open" class="tour-pop"></div>';
		const close = jest.fn();
		const { registerModal, unregisterModal } = useModalEscHandler();
		registerModal('m', close);
		document.body.dispatchEvent(esc());
		expect(close).toHaveBeenCalledTimes(1);
		unregisterModal('m');
	});
});
