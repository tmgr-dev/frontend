jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), put: jest.fn(), delete: jest.fn() },
}));

import axios from '@/plugins/axios';
import {
	alarmPhoneError,
	fetchAlarmPhone,
	isValidAlarmCode,
	isValidAlarmPhone,
	resendAlarmCode,
	removeAlarmPhone,
	saveAlarmPhone,
	startAlarmTestCall,
	verifyAlarmPhone,
} from '../alarmPhone';

const http = (status: number, data?: unknown, headers?: unknown) => ({
	response: { status, data, headers },
});

beforeEach(() => jest.clearAllMocks());

describe('isValidAlarmPhone', () => {
	it('accepts E.164 numbers', () => {
		expect(isValidAlarmPhone('+15550100123')).toBe(true);
		expect(isValidAlarmPhone(' +48500100123 ')).toBe(true);
	});

	it('rejects everything else', () => {
		for (const value of ['', '15550100123', '+0550100123', '+1555', '+1555010012345678', '+1 555 010 0123', '+1555abc0123']) {
			expect(isValidAlarmPhone(value)).toBe(false);
		}
	});
});

describe('fetchAlarmPhone', () => {
	it('unwraps the masked number', async () => {
		(axios.get as jest.Mock).mockResolvedValue({
			data: { data: { masked: '+1 ••• ••• 123', verified: false } },
		});
		expect(await fetchAlarmPhone()).toEqual({
			kind: 'set',
			phone: { masked: '+1 ••• ••• 123', verified: false },
		});
		expect(axios.get).toHaveBeenCalledWith('/user/alarm-phone');
	});

	it('maps 404 to the empty state', async () => {
		(axios.get as jest.Mock).mockRejectedValue(http(404));
		expect(await fetchAlarmPhone()).toEqual({ kind: 'unset' });
	});

	it('maps 503 to the not-configured state', async () => {
		(axios.get as jest.Mock).mockRejectedValue(http(503));
		expect(await fetchAlarmPhone()).toEqual({ kind: 'unconfigured' });
	});

	it('rethrows other errors', async () => {
		(axios.get as jest.Mock).mockRejectedValue(http(500));
		await expect(fetchAlarmPhone()).rejects.toEqual(http(500));
	});
});

describe('mutations', () => {
	it('saves a trimmed number and returns the masked form with codeSent', async () => {
		(axios.put as jest.Mock).mockResolvedValue({
			data: { data: { masked: '+1 ••• ••• 123', verified: false, codeSent: true } },
		});
		expect(await saveAlarmPhone(' +15550100123 ')).toEqual({
			masked: '+1 ••• ••• 123',
			verified: false,
			codeSent: true,
		});
		expect(axios.put).toHaveBeenCalledWith('/user/alarm-phone', { phone: '+15550100123' });
	});

	it('starts a test call and removes the phone', async () => {
		(axios.post as jest.Mock).mockResolvedValue({});
		(axios.delete as jest.Mock).mockResolvedValue({});
		await startAlarmTestCall();
		await removeAlarmPhone();
		expect(axios.post).toHaveBeenCalledWith('/user/alarm-phone/test-call');
		expect(axios.delete).toHaveBeenCalledWith('/user/alarm-phone');
	});
});

describe('verification', () => {
	it('sends a trimmed code and unwraps the verified phone', async () => {
		(axios.post as jest.Mock).mockResolvedValue({
			data: { data: { masked: '+1 ••• ••• 123', verified: true } },
		});
		expect(await verifyAlarmPhone(' 123456 ')).toEqual({
			masked: '+1 ••• ••• 123',
			verified: true,
		});
		expect(axios.post).toHaveBeenCalledWith('/user/alarm-phone/verify', { code: '123456' });
	});

	it('resends the code', async () => {
		(axios.post as jest.Mock).mockResolvedValue({});
		await resendAlarmCode();
		expect(axios.post).toHaveBeenCalledWith('/user/alarm-phone/resend');
	});

	it('validates 4-10 digit codes', () => {
		expect(isValidAlarmCode('1234')).toBe(true);
		expect(isValidAlarmCode('1234567890')).toBe(true);
		for (const value of ['', '123', '12345678901', '12a456']) {
			expect(isValidAlarmCode(value)).toBe(false);
		}
	});
});

describe('alarmPhoneError', () => {
	it('classifies HTTP statuses', () => {
		expect(alarmPhoneError(http(503))).toEqual({ kind: 'unconfigured' });
		expect(alarmPhoneError(http(400))).toEqual({ kind: 'invalid' });
		expect(alarmPhoneError(http(409))).toEqual({ kind: 'conflict' });
		expect(alarmPhoneError(http(500))).toEqual({ kind: 'failed' });
		expect(alarmPhoneError(new Error('network'))).toEqual({ kind: 'failed' });
	});

	it('reads attemptsLeft from data.attemptsLeft or attemptsLeft on 422', () => {
		expect(alarmPhoneError(http(422, { data: { attemptsLeft: 2 } }))).toEqual({
			kind: 'wrong_code',
			attemptsLeft: 2,
		});
		expect(alarmPhoneError(http(422, { attemptsLeft: 0 }))).toEqual({
			kind: 'wrong_code',
			attemptsLeft: 0,
		});
		expect(alarmPhoneError(http(422, {}))).toEqual({
			kind: 'wrong_code',
			attemptsLeft: undefined,
		});
	});

	it('maps 410 and 422 with reset to reset', () => {
		expect(alarmPhoneError(http(410))).toEqual({ kind: 'reset' });
		expect(alarmPhoneError(http(422, { reset: true }))).toEqual({ kind: 'reset' });
		expect(alarmPhoneError(http(422, { data: { reset: true } }))).toEqual({ kind: 'reset' });
	});

	it('parses Retry-After on 429 from plain and AxiosHeaders-like headers', () => {
		expect(alarmPhoneError(http(429, undefined, { 'retry-after': '42' }))).toEqual({
			kind: 'rate_limited',
			retryAfter: 42,
		});
		const headers = { get: (n: string) => (n === 'retry-after' ? '30' : undefined) };
		expect(alarmPhoneError(http(429, undefined, headers))).toEqual({
			kind: 'rate_limited',
			retryAfter: 30,
		});
		expect(alarmPhoneError(http(429))).toEqual({ kind: 'rate_limited', retryAfter: undefined });
		expect(alarmPhoneError(http(429, undefined, { 'retry-after': 'soon' }))).toEqual({
			kind: 'rate_limited',
			retryAfter: undefined,
		});
	});
});
