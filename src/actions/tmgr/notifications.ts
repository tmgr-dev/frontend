import $axios from 'src/plugins/axios';

export const sendNotification = async () => {
	await $axios.post('test/web/notifications');
};
