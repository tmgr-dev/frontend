import { format as dateFnsFormat } from 'date-fns';

const formatMappings: Record<string, string> = {
	'yyyy': 'yyyy',
	'yy': 'yy',
	'mm': 'MM',
	'dd': 'dd',
	'hh': 'HH',
	'h': 'H',
	'ii': 'mm',
	'i': 'm',
	'ss': 'ss',
	's': 's',
};

const convertFormat = (legacyFormat: string): string => {
	let converted = legacyFormat.toLowerCase();
	Object.entries(formatMappings).forEach(([legacy, dateFns]) => {
		converted = converted.replace(new RegExp(legacy, 'g'), dateFns);
	});
	return converted;
};

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
			.map((format) => format.split('#dte}')[0])
			.filter((format) => format.trim().length > 0)
			.forEach((format) => {
				datetimeMap.set(
					`{dts#${format}#dte}`,
					dateFnsFormat(new Date(), convertFormat(format)),
				);
			});
		datetimeMap.forEach((v, k) => {
			result = result.replaceAll(k, v);
		});
	}
	return result;
};
