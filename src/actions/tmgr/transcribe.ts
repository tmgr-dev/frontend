import $axios from '@/plugins/axios';

export const transcribeAudio = async (
	blob: Blob,
	mime: string,
	signal?: AbortSignal,
): Promise<string> => {
	const { data } = await $axios.post('/ai/transcribe', blob, {
		headers: { 'Content-Type': mime },
		timeout: 60000,
		signal,
	});
	return data?.data?.text ?? '';
};
