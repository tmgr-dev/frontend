import { detectUserTimeZone, resetTimeZoneDetection } from '../timezoneDetect';

const deps = (over: Record<string, unknown> = {}) => ({
	userId: 1,
	loggedIn: true,
	saved: undefined as unknown,
	detected: 'Europe/Warsaw' as string | null,
	save: jest.fn(async (tz: string) => tz),
	onSaved: jest.fn(),
	...over,
});

describe('detectUserTimeZone', () => {
	beforeEach(() => resetTimeZoneDetection());

	it('saves the detected zone once when nothing is stored', async () => {
		const d = deps();
		await detectUserTimeZone(d);
		await detectUserTimeZone(d);
		expect(d.save).toHaveBeenCalledTimes(1);
		expect(d.save).toHaveBeenCalledWith('Europe/Warsaw');
		expect(d.onSaved).toHaveBeenCalledWith('Europe/Warsaw');
	});

	it('does nothing when a zone is already saved', async () => {
		const d = deps({ saved: 'Asia/Tokyo' });
		await detectUserTimeZone(d);
		expect(d.save).not.toHaveBeenCalled();
	});

	it('does nothing when logged out or no zone is detected', async () => {
		const out = deps({ loggedIn: false });
		await detectUserTimeZone(out);
		const none = deps({ detected: null });
		await detectUserTimeZone(none);
		expect(out.save).not.toHaveBeenCalled();
		expect(none.save).not.toHaveBeenCalled();
	});

	it('does not retry after a failure', async () => {
		const d = deps({
			save: jest.fn(async () => {
				throw new Error('422');
			}),
		});
		await detectUserTimeZone(d);
		await detectUserTimeZone(d);
		expect(d.save).toHaveBeenCalledTimes(1);
		expect(d.onSaved).not.toHaveBeenCalled();
	});

	it('stores the zone the server returned', async () => {
		const d = deps({ save: jest.fn(async () => 'Europe/Berlin') });
		await detectUserTimeZone(d);
		expect(d.onSaved).toHaveBeenCalledWith('Europe/Berlin');
	});

	it('runs again for another user', async () => {
		const a = deps();
		const b = deps({ userId: 2 });
		await detectUserTimeZone(a);
		await detectUserTimeZone(b);
		expect(b.save).toHaveBeenCalledTimes(1);
	});
});
