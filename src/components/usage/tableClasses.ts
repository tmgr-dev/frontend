const stacked =
	'max-sm:flex max-sm:justify-between max-sm:gap-4 max-sm:text-right max-sm:before:text-left max-sm:before:text-ink-subtle max-sm:before:content-[attr(data-label)]';

export const cell = `py-1 sm:table-cell sm:py-2 sm:pr-3 ${stacked}`;

export const cellNum = `py-1 tabular-nums sm:table-cell sm:py-2 sm:pr-3 sm:text-right sm:last:pr-0 ${stacked}`;

export const rowClass =
	'block border-t border-border py-2 sm:table-row sm:py-0';

export const headClass =
	'hidden text-left text-xs text-ink-subtle sm:table-header-group';
