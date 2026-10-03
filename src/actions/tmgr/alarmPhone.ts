import $axios from '@/plugins/axios';

export interface AlarmPhone {
	masked: string;
	verified: boolean;
}

export type AlarmPhoneState =
	| { kind: 'unset' }
	| { kind: 'set'; phone: AlarmPhone }
	| { kind: 'unconfigured' };

export type AlarmPhoneFailure =
	| 'unconfigured'
	| 'invalid'
	| 'rate_limited'
	| 'failed';

const E164 = /^\+[1-9]\d{7,14}$/;

export const isValidAlarmPhone = (value: string): boolean =>
	E164.test(value.trim());

export const alarmPhoneFailure = (error: unknown): AlarmPhoneFailure => {
	switch ((error as any)?.response?.status) {
		case 503:
			return 'unconfigured';
		case 400:
			return 'invalid';
		case 429:
			return 'rate_limited';
		default:
			return 'failed';
	}
};

export const fetchAlarmPhone = async (): Promise<AlarmPhoneState> => {
	try {
		const {
			data: { data },
		} = await $axios.get('/user/alarm-phone');
		return { kind: 'set', phone: data };
	} catch (error) {
		const failure = (error as any)?.response?.status;
		if (failure === 404) return { kind: 'unset' };
		if (failure === 503) return { kind: 'unconfigured' };
		throw error;
	}
};

export const saveAlarmPhone = async (phone: string): Promise<AlarmPhone> => {
	const {
		data: { data },
	} = await $axios.put('/user/alarm-phone', { phone: phone.trim() });
	return data;
};

export const startAlarmTestCall = async (): Promise<void> => {
	await $axios.post('/user/alarm-phone/test-call');
};

export const removeAlarmPhone = async (): Promise<void> => {
	await $axios.delete('/user/alarm-phone');
};
