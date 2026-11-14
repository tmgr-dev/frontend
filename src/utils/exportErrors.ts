export const isExportFeatureDisabled = async (error: any): Promise<boolean> => {
	const response = error?.response;
	if (response?.status !== 403) return false;
	let data = response.data;
	if (typeof Blob !== 'undefined' && data instanceof Blob) {
		try {
			data = JSON.parse(await data.text());
		} catch {
			return false;
		}
	}
	return data?.error === 'feature_disabled';
};
