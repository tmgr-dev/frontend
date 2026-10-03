import $axios from '@/plugins/axios';

export interface AlarmPhone {
	masked: string;
	verified: boolean;
}

export type AlarmPhoneState =
	| { kind: 'unset' }
	| { kind: 'set'; phone: AlarmPhone }
	| { kind: 'unconfigured' };

export interface AlarmPhoneSaved extends AlarmPhone {
	codeSent?: boolean;
}

export type AlarmPhoneFailure =
	| 'unconfigured'
	| 'invalid'
	| 'rate_limited'
	| 'wrong_code'
	| 'reset'
	| 'conflict'
	| 'failed';

export interface AlarmPhoneError {
	kind: AlarmPhoneFailure;
	attemptsLeft?: number;
	retryAfter?: number;
}

const E164 = /^\+[1-9]\d{7,14}$/;

export const isValidAlarmPhone = (value: string): boolean =>
	E164.test(value.trim());

const readHeader = (headers: any, name: string): string | undefined => {
	if (!headers) return undefined;
	const value =
		typeof headers.get === 'function' ? headers.get(name) : headers[name];
	return value === undefined || value === null ? undefined : String(value);
};

const toCount = (value: unknown): number | undefined => {
	const n = Number(value);
	return value !== undefined && value !== null && value !== '' && Number.isFinite(n) && n >= 0
		? Math.floor(n)
		: undefined;
};

export const alarmPhoneError = (error: unknown): AlarmPhoneError => {
	const response = (error as any)?.response;
	const body = response?.data;
	switch (response?.status) {
		case 503:
			return { kind: 'unconfigured' };
		case 400:
			return { kind: 'invalid' };
		case 409:
			return { kind: 'conflict' };
		case 410:
			return { kind: 'reset' };
		case 429:
			return {
				kind: 'rate_limited',
				retryAfter: toCount(readHeader(response.headers, 'retry-after')),
			};
		case 422: {
			if (body?.reset === true || body?.data?.reset === true) {
				return { kind: 'reset' };
			}
			return {
				kind: 'wrong_code',
				attemptsLeft: toCount(body?.data?.attemptsLeft ?? body?.attemptsLeft),
			};
		}
		default:
			return { kind: 'failed' };
	}
};

export const fetchAlarmPhone = async (): Promise<AlarmPhoneState> => {
	try {
		const {
			data: { data },
		} = await $axios.get('/user/alarm-phone');
		return { kind: 'set', phone: data };
	} catch (error) {
		const status = (error as any)?.response?.status;
		if (status === 404) return { kind: 'unset' };
		if (status === 503) return { kind: 'unconfigured' };
		throw error;
	}
};

export const saveAlarmPhone = async (phone: string): Promise<AlarmPhoneSaved> => {
	const {
		data: { data },
	} = await $axios.put('/user/alarm-phone', { phone: phone.trim() });
	return data;
};

export const verifyAlarmPhone = async (code: string): Promise<AlarmPhone> => {
	const {
		data: { data },
	} = await $axios.post('/user/alarm-phone/verify', { code: code.trim() });
	return data;
};

export const resendAlarmCode = async (): Promise<void> => {
	await $axios.post('/user/alarm-phone/resend');
};

export const isValidAlarmCode = (value: string): boolean =>
	/^\d{4,10}$/.test(value.trim());

export const startAlarmTestCall = async (): Promise<void> => {
	await $axios.post('/user/alarm-phone/test-call');
};

export const removeAlarmPhone = async (): Promise<void> => {
	await $axios.delete('/user/alarm-phone');
};
