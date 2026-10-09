import { createFeatureDisabledNotifier } from '../featureDisabledNotice';

const disabled = (message: string) => ({
	response: { status: 403, data: { error: 'feature_disabled', message } },
});

describe('createFeatureDisabledNotifier', () => {
	it('shows the server message', () => {
		const show = jest.fn();
		createFeatureDisabledNotifier(show)(disabled('Module `pomodoro` is off'));
		expect(show).toHaveBeenCalledWith('Module `pomodoro` is off');
	});

	it('ignores other errors', () => {
		const show = jest.fn();
		const notify = createFeatureDisabledNotifier(show);
		notify({ response: { status: 403, data: { message: 'Forbidden' } } });
		notify(new Error('x'));
		expect(show).not.toHaveBeenCalled();
	});

	it('does not repeat the same message in quick succession', () => {
		const show = jest.fn();
		let time = 0;
		const notify = createFeatureDisabledNotifier(show, () => time);
		notify(disabled('a'));
		time = 1000;
		notify(disabled('a'));
		notify(disabled('b'));
		time = 6000;
		notify(disabled('b'));
		expect(show.mock.calls.map((c) => c[0])).toEqual(['a', 'b', 'b']);
	});
});
