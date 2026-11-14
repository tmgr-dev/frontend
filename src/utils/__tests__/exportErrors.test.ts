import { isExportFeatureDisabled } from '../exportErrors';

const forbidden = (data: unknown) => ({ response: { status: 403, data } });
const body = { error: 'feature_disabled', feature: 'exports' };

describe('isExportFeatureDisabled', () => {
	it('detects feature_disabled in a blob body', async () => {
		const blob = new Blob([JSON.stringify(body)], {
			type: 'application/json',
		});
		expect(await isExportFeatureDisabled(forbidden(blob))).toBe(true);
	});

	it('detects feature_disabled in a parsed body', async () => {
		expect(await isExportFeatureDisabled(forbidden(body))).toBe(true);
	});

	it('ignores other 403 codes, other statuses and unreadable bodies', async () => {
		expect(
			await isExportFeatureDisabled(forbidden({ error: 'forbidden' })),
		).toBe(false);
		expect(
			await isExportFeatureDisabled({ response: { status: 500, data: body } }),
		).toBe(false);
		expect(await isExportFeatureDisabled(forbidden(new Blob(['nope'])))).toBe(
			false,
		);
		expect(await isExportFeatureDisabled(new Error('x'))).toBe(false);
	});
});
