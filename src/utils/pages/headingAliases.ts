const GROUPS: string[][] = [
	['timeline', 'хронология'],
	['summary', 'кратко'],
	['promises', 'обещания'],
	['what i know', 'что я знаю'],
	['insights', 'инсайты'],
	['agenda', 'повестка'],
	['outcomes', 'итоги'],
	['decisions', 'решения'],
	['action items', 'действия'],
	['how we work', 'как мы работаем'],
	['architecture', 'архитектура'],
	['agent notes', 'заметки агентов'],
];

const normalize = (heading: string): string =>
	heading.trim().replace(/\s+/g, ' ').toLowerCase();

export const headingAliases = (heading: string): string[] => {
	const key = normalize(heading);
	return GROUPS.find((group) => group.includes(key)) ?? [key];
};

export const headingsMatch = (a: string, b: string): boolean =>
	headingAliases(a).includes(normalize(b));
