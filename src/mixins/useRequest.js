import { ref } from 'vue';

/*
 * @todo think about global try/catch. This method is not suitable
 * */

export default function useRequest(asyncMethod) {
	const result = ref();
	const isLoading = ref(false);
	const message = ref('');
	const errors = ref({});

	const handleResponse = async (...args) => {
		try {
			isLoading.value = true;
			const { data } = await asyncMethod(...args);
			result.value = data;
		} catch (error) {
			errors.value = error.response?.data?.errors;
			message.value = error.response?.data?.message;
		} finally {
			isLoading.value = false;
		}
	};

	return {
		result,
		message,
		errors,
		isLoading,
		handleResponse,
	};
}
