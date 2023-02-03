export default function objectToQueryString(obj, prefix) {
	const isNotNullObject = (v) => v !== null && typeof v === 'object';
	const queryStringItems = [];

	Object.keys(obj).forEach((key) => {
		const processedKey = prefix ? prefix + '[' + key + ']' : key;
		const value = obj[key];

		queryStringItems.push(
			isNotNullObject(value)
				? objectToQueryString(value, processedKey)
				: encodeURIComponent(processedKey) + '=' + encodeURIComponent(value),
		);
	});

	return queryStringItems.join('&');
}
