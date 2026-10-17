export const SNIPPET_ROW_CLASS =
	'flex flex-col gap-2 sm:flex-row sm:items-start';

export const SNIPPET_BUTTON_CLASS = 'shrink-0 self-end sm:self-start';

export function snippetTextClass(wrap: boolean): string {
	return wrap
		? 'min-w-0 flex-1 whitespace-pre-wrap [overflow-wrap:anywhere]'
		: 'min-w-0 flex-1 overflow-x-auto whitespace-pre';
}
