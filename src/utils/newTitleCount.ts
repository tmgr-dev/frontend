export function newTitleCount(title: string) {
	const prefix = title.split('-')[0];
	const num = parseInt(title.split('-')[1]) + 1;
	return `${prefix}-${num}:`;
}
