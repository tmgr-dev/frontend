import $axios from 'src/plugins/axios';

export const getStats = async () => {
	const {
		data: { data },
	} = await $axios.get('stats');

	return data;
};
