jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), put: jest.fn(), delete: jest.fn() },
}));

import axios from '@/plugins/axios';
import {
	alarmPhoneFailure,
	fetchAlarmPhone,
	isValidAlarmPhone,
	removeAlarmPhone,
	saveAlarmPhone,
	startAlarmTestCall,
} from '../alarmPhone';

const http = (status: number) => ({ response: { status } });

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
	it('saves a trimmed number and returns only the masked form', async () => {
		(axios.put as jest.Mock).mockResolvedValue({
			data: { data: { masked: '+1 ••• ••• 123', verified: false } },
		});
		expect(await saveAlarmPhone(' +15550100123 ')).toEqual({
			masked: '+1 ••• ••• 123',
			verified: false,
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

describe('alarmPhoneFailure', () => {
	it('classifies HTTP statuses', () => {
		expect(alarmPhoneFailure(http(503))).toBe('unconfigured');
		expect(alarmPhoneFailure(http(400))).toBe('invalid');
		expect(alarmPhoneFailure(http(429))).toBe('rate_limited');
		expect(alarmPhoneFailure(http(500))).toBe('failed');
		expect(alarmPhoneFailure(new Error('network'))).toBe('failed');
	});
});
