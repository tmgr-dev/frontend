import {
	browserTimeZone,
	currentTimeZone,
	isValidTimeZone,
	setTimeZoneSource,
	timeZoneOptions,
	todayDateIn,
	todayIn,
	wallNowIn,
} from '../timezone';

describe('isValidTimeZone', () => {
	it('accepts IANA ids and rejects junk', () => {
		expect(isValidTimeZone('Europe/Warsaw')).toBe(true);
		expect(isValidTimeZone('UTC')).toBe(true);
		expect(isValidTimeZone('Nope/Zone')).toBe(false);
		expect(isValidTimeZone('')).toBe(false);
		expect(isValidTimeZone(null)).toBe(false);
		expect(isValidTimeZone(undefined)).toBe(false);
	});
});

describe('browserTimeZone', () => {
	const original = Intl.DateTimeFormat;
	const stub = (timeZone: string | undefined) => {
		(Intl as any).DateTimeFormat = function () {
			return { resolvedOptions: () => ({ timeZone }) };
		};
	};
	afterEach(() => {
		(Intl as any).DateTimeFormat = original;
	});

	it('returns the resolved zone', () => {
		stub('Asia/Tokyo');
		expect(browserTimeZone()).toBe('Asia/Tokyo');
	});

	it('returns null for empty or Etc/Unknown', () => {
		stub('');
		expect(browserTimeZone()).toBeNull();
		stub(undefined);
		expect(browserTimeZone()).toBeNull();
		stub('Etc/Unknown');
		expect(browserTimeZone()).toBeNull();
	});
});

describe('todayIn / wallNowIn', () => {
	it('follows the zone across the Tokyo day boundary', () => {
		const at = new Date('2026-06-10T15:30:00Z');
		expect(todayIn('UTC', at)).toBe('2026-06-10');
		expect(todayIn('Asia/Tokyo', at)).toBe('2026-06-11');
		expect(wallNowIn('Asia/Tokyo', at)).toEqual({
			date: '2026-06-11',
			hours: 0,
			minutes: 30,
		});
	});

	it('handles the Warsaw spring-forward switch', () => {
		expect(
			wallNowIn('Europe/Warsaw', new Date('2026-03-29T00:30:00Z')),
		).toEqual({ date: '2026-03-29', hours: 1, minutes: 30 });
		expect(
			wallNowIn('Europe/Warsaw', new Date('2026-03-29T01:30:00Z')),
		).toEqual({ date: '2026-03-29', hours: 3, minutes: 30 });
	});

	it('handles the Warsaw fall-back switch', () => {
		expect(
			wallNowIn('Europe/Warsaw', new Date('2026-10-24T22:30:00Z')),
		).toEqual({ date: '2026-10-25', hours: 0, minutes: 30 });
		expect(
			wallNowIn('Europe/Warsaw', new Date('2026-10-25T00:30:00Z')),
		).toEqual({ date: '2026-10-25', hours: 2, minutes: 30 });
		expect(
			wallNowIn('Europe/Warsaw', new Date('2026-10-25T01:30:00Z')),
		).toEqual({ date: '2026-10-25', hours: 2, minutes: 30 });
		expect(
			wallNowIn('Europe/Warsaw', new Date('2026-10-25T23:30:00Z')),
		).toEqual({ date: '2026-10-26', hours: 0, minutes: 30 });
	});

	it('renders midnight as hour 0', () => {
		expect(wallNowIn('UTC', new Date('2026-01-01T00:05:00Z'))).toEqual({
			date: '2026-01-01',
			hours: 0,
			minutes: 5,
		});
	});
});

describe('todayDateIn', () => {
	it('is local midnight of the zone date', () => {
		const d = todayDateIn('Asia/Tokyo', new Date('2026-06-10T15:30:00Z'));
		expect([d.getFullYear(), d.getMonth(), d.getDate()]).toEqual([2026, 5, 11]);
		expect([d.getHours(), d.getMinutes()]).toEqual([0, 0]);
	});
});

describe('timeZoneOptions', () => {
	it('is sorted, labelled with the offset and keeps the current zone', () => {
		const at = new Date('2026-07-01T12:00:00Z');
		const options = timeZoneOptions('Europe/Warsaw', at);
		const warsaw = options.find((o) => o.value === 'Europe/Warsaw');
		expect(warsaw?.label).toBe('Europe/Warsaw (UTC+02:00)');
		const values = options.map((o) => o.value);
		expect([...values].sort()).toEqual(values);
	});

	it('adds a valid current zone that Intl lists only under another name', () => {
		const options = timeZoneOptions('Asia/Calcutta', new Date());
		expect(options.some((o) => o.value === 'Asia/Calcutta')).toBe(true);
	});

	it('labels negative and half-hour offsets', () => {
		const at = new Date('2026-01-15T12:00:00Z');
		const options = timeZoneOptions('Asia/Kolkata', at);
		expect(options.find((o) => o.value === 'Asia/Kolkata')?.label).toBe(
			'Asia/Kolkata (UTC+05:30)',
		);
		expect(
			timeZoneOptions('America/New_York', at).find(
				(o) => o.value === 'America/New_York',
			)?.label,
		).toBe('America/New_York (UTC-05:00)');
	});
});

describe('currentTimeZone', () => {
	afterEach(() => setTimeZoneSource(null));

	it('uses the registered source', () => {
		setTimeZoneSource(() => 'Asia/Tokyo');
		expect(currentTimeZone()).toBe('Asia/Tokyo');
	});

	it('falls back to the browser zone', () => {
		expect(currentTimeZone()).toBe(browserTimeZone() || 'UTC');
	});
});
