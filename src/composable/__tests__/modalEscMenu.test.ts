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

describe('useModalEscHandler with an open menu', () => {
	let now = 0;
	beforeEach(() => {
		now += 10_000;
		jest.spyOn(Date, 'now').mockReturnValue(now);
	});
	afterEach(() => {
		jest.restoreAllMocks();
		document.body.innerHTML = '';
	});

	it.each(['menu', 'listbox'])(
		'lets an open %s take the Esc, then closes on the next one',
		(role) => {
			document.body.innerHTML = `<div role="${role}" data-state="open"></div>`;
			const close = jest.fn();
			const { registerModal, unregisterModal } = useModalEscHandler();
			registerModal('m', close);

			const first = esc();
			document.body.dispatchEvent(first);
			expect(close).not.toHaveBeenCalled();
			expect(first.defaultPrevented).toBe(false);

			document.body.innerHTML = '';
			document.body.dispatchEvent(esc());
			expect(close).toHaveBeenCalledTimes(1);
			unregisterModal('m');
		},
	);

	it('closes the modal when the menu is only closing', () => {
		document.body.innerHTML = '<div role="menu" data-state="closed"></div>';
		const close = jest.fn();
		const { registerModal, unregisterModal } = useModalEscHandler();
		registerModal('m', close);
		document.body.dispatchEvent(esc());
		expect(close).toHaveBeenCalledTimes(1);
		unregisterModal('m');
	});
});
