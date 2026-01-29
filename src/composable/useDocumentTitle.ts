const APP_NAME = 'TMGR';
const SEPARATOR = ' | ';

export function setDocumentTitle(pageTitle?: string): void {
	document.title = pageTitle ? `${pageTitle}${SEPARATOR}${APP_NAME}` : APP_NAME;
}

export function useDocumentTitle() {
	return {
		setTitle: setDocumentTitle,
		APP_NAME,
		SEPARATOR,
	};
}

export { APP_NAME, SEPARATOR };
