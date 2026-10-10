import {
	createFeatureDisabledNotifier,
	describeFeatureDisabled,
} from '../featureDisabledNotice';

const disabled = (
	method: string,
	feature = 'daily_routines',
	message = `Module \`${feature}\` is off in workspace \`ws\``,
) => ({
	config: { method },
	response: {
		status: 403,
		data: { error: 'feature_disabled', feature, message },
	},
});

describe('describeFeatureDisabled', () => {
	it('is friendly and names the module for a user-initiated write', () => {
		const notice = describeFeatureDisabled(disabled('post'), () => 'Routines');
		expect(notice).toEqual({
			title: 'Routines is turned off',
			description:
				'Routines is turned off in this workspace. Turn it on in Settings → Modules.',
		});
	});

	it('says "for your account" for user-scoped modules', () => {
		const notice = describeFeatureDisabled(
			disabled('put', 'alerts', 'Module `alerts` is off for your account'),
			() => 'Agent alerts',
		);
		expect(notice?.description).toContain('for your account');
	});

	it('falls back to a readable key when the catalog has no name', () => {
		expect(describeFeatureDisabled(disabled('post'))?.title).toBe(
			'Daily routines is turned off',
		);
	});

	it('never produces a notice for reads', () => {
		expect(describeFeatureDisabled(disabled('get'))).toBeNull();
		expect(
			describeFeatureDisabled({ response: disabled('x').response }),
		).toBeNull();
	});

	it('ignores other errors', () => {
		expect(
			describeFeatureDisabled({
				config: { method: 'post' },
				response: { status: 403, data: {} },
			}),
		).toBeNull();
		expect(describeFeatureDisabled(new Error('x'))).toBeNull();
	});
});

describe('createFeatureDisabledNotifier', () => {
	it('shows a notice for writes only', () => {
		const show = jest.fn();
		const notify = createFeatureDisabledNotifier(show);
		notify(disabled('get'));
		expect(show).not.toHaveBeenCalled();
		notify(disabled('post'));
		expect(show).toHaveBeenCalledTimes(1);
	});

	it('refreshes modules silently after a server-side read refusal, rate limited', () => {
		const onSilent = jest.fn();
		let time = 0;
		const notify = createFeatureDisabledNotifier(
			jest.fn(),
			() => time,
			undefined,
			onSilent,
		);
		notify(disabled('get'));
		notify(disabled('get'));
		expect(onSilent).toHaveBeenCalledTimes(1);
		time = 6000;
		notify(disabled('get'));
		expect(onSilent).toHaveBeenCalledTimes(2);
	});

	it('does not refresh for refusals produced by the client itself', () => {
		const onSilent = jest.fn();
		const notify = createFeatureDisabledNotifier(
			jest.fn(),
			Date.now,
			undefined,
			onSilent,
		);
		notify({ ...disabled('get'), clientModuleGate: true });
		expect(onSilent).not.toHaveBeenCalled();
	});

	it('does not repeat the same notice in quick succession', () => {
		const show = jest.fn();
		let time = 0;
		const notify = createFeatureDisabledNotifier(show, () => time);
		notify(disabled('post', 'a'));
		time = 1000;
		notify(disabled('post', 'a'));
		notify(disabled('post', 'b'));
		time = 6000;
		notify(disabled('post', 'b'));
		expect(show).toHaveBeenCalledTimes(3);
	});
});
