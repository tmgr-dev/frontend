import $axios from '@/plugins/axios';

export const generateSmartDeviceToken = async () => {
	const { data } = await $axios.post('/smart-device/token/generate');
	return data;
};

export const revokeSmartDeviceToken = async () => {
	const { data } = await $axios.post('/smart-device/token/revoke');
	return data;
};

export const verifySmartDeviceToken = async () => {
	const { data } = await $axios.post('/smart-device/token/verify');
	return data;
};
