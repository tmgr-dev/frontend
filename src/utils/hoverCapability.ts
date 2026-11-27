export const deviceCanHover = (
	win: Pick<Window, 'matchMedia'> | undefined = typeof window === 'undefined'
		? undefined
		: window,
): boolean =>
	typeof win?.matchMedia !== 'function' ||
	win.matchMedia('(hover: hover)').matches;
