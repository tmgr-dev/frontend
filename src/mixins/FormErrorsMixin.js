import { ref } from 'vue';
export default function () {
	let message = ref('');
	let errors = ref({});

	const showErrorsFromResponse = ({data}) => {
		errors.value = data.errors;
		message.value = data.message;
	}

	const tryCatchRequest = async (requestCallback) => {
		try {
			await requestCallback();
		} catch (error) {
			showErrorsFromResponse(error.response);
		}
	}

	return {
		message,
		errors,
		tryCatchRequest,
		showErrorsFromResponse
	};
};
