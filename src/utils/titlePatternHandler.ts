import moment from 'moment';
/*
	#Pattern rules:

	## Index:
	{index#[type of index]}
	[type of index]:
		1. workspace = Index will generated based on number of tasks in the workspace
		2. category = Index will generated based on number of tasks in the category

	## Date Time:
	{datetime#[]}
		Format of date time pattern is the ISO 8601(https://www.w3.org/TR/NOTE-datetime)
		ex.: YYYY-MM-DD
	_____________________
	Example:

	Input: "TMGR-{index#workspace}: {dts#H:S:i#dte}"
	Output: "TMGR-1: 22:12:01"
 */
export const titlePatternHandler = (
	rawString: string,
	indexes: Map<string, number> = new Map([
		['workspace', 0],
		['category', 0],
	]),
): string => {
	let result = rawString;
	if (result.includes('{index#')) {
		indexes.forEach((index, indexKey) => {
			result = result.replaceAll(`{index#${indexKey}}`, String(index));
		});
	}
	let datetimeMap: Map<string, string> = new Map();
	if (result.includes('{dts#')) {
		let datetimes = result.split('{dts#');
		datetimes
			.map((item) => item.split('#dte}')[0])
			.filter((item) => item.trim().length > 0)
			.forEach((item) => {
				console.log(item);
				datetimeMap.set(`{dts#${item}#dte}`, moment().format(item));
			});
		datetimeMap.forEach((v, k) => {
			result = result.replaceAll(k, v);
		});
	}
	return result;
};
