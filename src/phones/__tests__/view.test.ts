import type { PairingView } from '../types';
import {
	attemptsLabel,
	countdownLabel,
	pairingHeadline,
	phonesAvailable,
	shortFingerprint,
	sinceLabel,
} from '../view';

const pairing = (over: Partial<PairingView>): PairingView => ({
	state: 'waiting',
	qr_svg_data_url: 'data:image/svg+xml;base64,AAAA',
	expires_ms: 0,
	attempts_left: 3,
	...over,
});

describe('phones view helpers', () => {
	it('renders each pairing state', () => {
		expect(pairingHeadline(pairing({ state: 'waiting' }))).toMatch(/Scan/);
		expect(pairingHeadline(pairing({ state: 'confirm' }))).toBe(
			'Type the code shown on your iPhone',
		);
		expect(
			pairingHeadline(pairing({ state: 'confirm', phone_name: 'Ann iPhone' })),
		).toBe('Type the code shown on Ann iPhone');
		expect(
			pairingHeadline(pairing({ state: 'failed', error: 'Code expired' })),
		).toBe('Code expired');
		expect(pairingHeadline(pairing({ state: 'failed' }))).toBe(
			'Pairing failed',
		);
		expect(pairingHeadline(pairing({ state: 'done', phone_name: 'Ann' }))).toBe(
			'Ann is paired',
		);
	});

	it('formats the countdown and clamps at zero', () => {
		expect(countdownLabel(5 * 60_000, 0)).toBe('5:00');
		expect(countdownLabel(61_500, 0)).toBe('1:02');
		expect(countdownLabel(1000, 5000)).toBe('0:00');
	});

	it('formats attempts, fingerprint and last used', () => {
		expect(attemptsLabel(1)).toBe('1 attempt left');
		expect(attemptsLabel(2)).toBe('2 attempts left');
		expect(shortFingerprint('abcdef0123456789')).toBe('ABCDEF01');
		expect(sinceLabel(0, 10)).toBe('never');
		expect(sinceLabel(0 + 1, 1 + 3 * 3600_000)).toBe('3 h ago');
	});

	it('is available only on desktop with an account', () => {
		expect(phonesAvailable(true, false)).toBe(true);
		expect(phonesAvailable(false, false)).toBe(false);
		expect(phonesAvailable(true, true)).toBe(false);
	});
});
