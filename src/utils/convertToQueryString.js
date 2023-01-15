function convertToQueryString(obj, prefix) {
	const euc = encodeURIComponent;
	const serialize = convertToQueryString;
	const isNotNullObject = (v) => v !== null && typeof v === 'object';
	const queryStringItems = [];

	for (let p in obj) {
		if (!obj.hasOwnProperty(p)) {
			continue;
		}

		const k = prefix ? prefix + '[' + p + ']' : p;
		const v = obj[p];
		queryStringItems.push(
			isNotNullObject(v) ? serialize(v, k) : euc(k) + '=' + euc(v),
		);
	}
	return queryStringItems.join('&');
}

export default convertToQueryString;
