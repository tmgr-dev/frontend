export const isDesktopApp = (): boolean =>
	typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const desktopWindowLabel = (): string | null =>
	(window as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? null;

export function installAutoHideScrollbars(
	doc: Document = document,
	hideAfterMs = 900,
): void {
	const timers = new WeakMap<Element, ReturnType<typeof setTimeout>>();
	doc.addEventListener(
		'scroll',
		(event) => {
			const el =
				event.target instanceof Element ? event.target : doc.documentElement;
			el.classList.add('is-scrolling');
			clearTimeout(timers.get(el));
			timers.set(
				el,
				setTimeout(() => el.classList.remove('is-scrolling'), hideAfterMs),
			);
		},
		{ capture: true, passive: true },
	);
}
