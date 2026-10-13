import { createController, scrollIndicatorText } from '../controller';

const setup = () => {
	const writes: { data: string; done: () => void }[] = [];
	const term = {
		write: jest.fn((data: string, done?: () => void) => {
			writes.push({ data, done: done ?? (() => {}) });
		}),
		rows: 30,
	};
	const api = {
		ack: jest.fn().mockResolvedValue(undefined),
		input: jest.fn().mockResolvedValue(undefined),
		scroll: jest.fn().mockResolvedValue(undefined),
		detach: jest.fn().mockResolvedValue(undefined),
		openLink: jest.fn().mockResolvedValue(undefined),
	};
	const view = {
		setHeader: jest.fn(),
		setScrollText: jest.fn(),
		showClosed: jest.fn(),
		hideClosed: jest.fn(),
		closeWindow: jest.fn(),
	};
	const controller = createController({ term, api, view });
	return { term, api, view, controller, writes };
};

describe('terminal controller', () => {
	it('acks a data frame only after the write completed', () => {
		const { controller, api, writes } = setup();
		controller.handleFrame({ type: 'data', seq: 3, data: '\x1b[Hhi' });
		expect(writes[0].data).toBe('\x1b[Hhi');
		expect(api.ack).not.toHaveBeenCalled();
		writes[0].done();
		expect(api.ack).toHaveBeenCalledWith(3);
	});

	it('shows the state label and highlights Needs input', () => {
		const { controller, view } = setup();
		controller.handleFrame({
			type: 'state',
			state: { kind: 'blocked', sinceMs: 1, detail: 'Allow edit?' },
		});
		expect(view.setHeader).toHaveBeenLastCalledWith({
			label: 'Needs input',
			tone: 'attention',
			highlight: true,
			detail: 'Allow edit?',
		});
		controller.handleFrame({
			type: 'state',
			state: { kind: 'working', sinceMs: 2, detail: null },
		});
		expect(view.setHeader).toHaveBeenLastCalledWith(
			expect.objectContaining({ highlight: false, detail: null }),
		);
	});

	it('shows a scroll indicator only while scrolled back', () => {
		const { controller, view } = setup();
		controller.handleFrame({ type: 'scroll', offset: 12, max: 100 });
		expect(view.setScrollText).toHaveBeenLastCalledWith(
			'Scrolled back 12 lines — press End to return',
		);
		controller.handleFrame({ type: 'scroll', offset: 0, max: 100 });
		expect(view.setScrollText).toHaveBeenLastCalledWith(null);
		expect(scrollIndicatorText(1)).toBe(
			'Scrolled back 1 line — press End to return',
		);
	});

	it('shows the closed overlay and ignores input afterwards', () => {
		const { controller, view, api } = setup();
		controller.handleFrame({ type: 'closed', reason: 'runner stopped' });
		expect(view.showClosed).toHaveBeenCalledWith('runner stopped');
		controller.onInput('x');
		expect(api.input).not.toHaveBeenCalled();
		controller.reopened();
		expect(view.hideClosed).toHaveBeenCalled();
		controller.onInput('x');
		expect(api.input).toHaveBeenCalledWith('x');
	});

	it('scrolls with the wheel, clamped to the known range', () => {
		const { controller, api } = setup();
		controller.handleFrame({ type: 'scroll', offset: 0, max: 5 });
		controller.wheel(-100);
		expect(api.scroll).toHaveBeenLastCalledWith(3);
		controller.handleFrame({ type: 'scroll', offset: 3, max: 5 });
		controller.wheel(-100);
		expect(api.scroll).toHaveBeenLastCalledWith(5);
		controller.handleFrame({ type: 'scroll', offset: 5, max: 5 });
		controller.wheel(100);
		expect(api.scroll).toHaveBeenLastCalledWith(2);
	});

	it('handles Shift+PageUp/PageDown and End, passing other keys through', () => {
		const { controller, api } = setup();
		controller.handleFrame({ type: 'scroll', offset: 0, max: 200 });
		expect(controller.handleKey({ key: 'PageUp', shiftKey: true })).toBe(true);
		expect(api.scroll).toHaveBeenLastCalledWith(29);
		controller.handleFrame({ type: 'scroll', offset: 29, max: 200 });
		expect(controller.handleKey({ key: 'PageDown', shiftKey: true })).toBe(
			true,
		);
		expect(api.scroll).toHaveBeenLastCalledWith(0);
		controller.handleFrame({ type: 'scroll', offset: 10, max: 200 });
		expect(controller.handleKey({ key: 'End', shiftKey: false })).toBe(true);
		expect(api.scroll).toHaveBeenLastCalledWith(0);
		expect(controller.handleKey({ key: 'PageUp', shiftKey: false })).toBe(
			false,
		);
		expect(controller.handleKey({ key: 'a', shiftKey: true })).toBe(false);
	});

	it('End does not swallow the key when already at the bottom', () => {
		const { controller } = setup();
		expect(controller.handleKey({ key: 'End', shiftKey: false })).toBe(false);
	});

	it('detach tells the host and closes the window', async () => {
		const { controller, api, view } = setup();
		await controller.detach();
		expect(api.detach).toHaveBeenCalled();
		expect(view.closeWindow).toHaveBeenCalled();
	});

	it('opens links only through the host', () => {
		const { controller, api } = setup();
		controller.openLink('https://example.com');
		expect(api.openLink).toHaveBeenCalledWith('https://example.com');
	});
});
