export const uploadPendingFiles = async <T>(
	files: File[],
	upload: (file: File) => Promise<T>,
): Promise<{ attached: T[]; failed: File[] }> => {
	const results = await Promise.allSettled(files.map((file) => upload(file)));
	const attached: T[] = [];
	const failed: File[] = [];

	results.forEach((result, index) => {
		if (result.status === 'fulfilled') {
			attached.push(result.value);
		} else {
			failed.push(files[index]);
		}
	});

	return { attached, failed };
};
