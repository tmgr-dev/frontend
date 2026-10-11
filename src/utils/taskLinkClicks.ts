import { parseTaskLink } from './taskLinks';

interface ClickLike {
	button: number;
	metaKey: boolean;
	ctrlKey: boolean;
	shiftKey: boolean;
	altKey: boolean;
	defaultPrevented: boolean;
}

export const OPEN_APP_LINK_EVENT = 'open-app-link';

export const isPlainTaskLinkClick = (
	event: ClickLike,
	href: string | null | undefined,
): boolean =>
	!event.defaultPrevented &&
	event.button === 0 &&
	!event.metaKey &&
	!event.ctrlKey &&
	!event.shiftKey &&
	!event.altKey &&
	parseTaskLink(href || '') !== null;

export const installTaskLinkClicks = (
	send: (url: string) => Promise<unknown>,
): void => {
	document.addEventListener('click', (event) => {
		const anchor = (event.target as Element | null)?.closest?.('a[href]');
		const href = anchor?.getAttribute('href');
		if (!href || !isPlainTaskLinkClick(event, href)) return;
		event.preventDefault();
		void send(href).catch(() => {});
	});
};
