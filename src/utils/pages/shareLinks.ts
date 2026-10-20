import type { Page } from '@/actions/tmgr/pages';
import { PAGE_LINK } from '@/utils/desktopShortcuts';
import { pageUrl } from '@/utils/pagesTree';
import { WEB_ORIGIN } from '@/utils/taskLinks';

export interface PageShareLinks {
	web: string | null;
	app: string | null;
}

export const pageShareLinks = (
	page: { slug: string },
	workspace: { id: number | string; code: string },
): PageShareLinks => {
	const app = `tmgr://page/${workspace.code}/${page.slug}`;
	return {
		web:
			Number(workspace.id) < 0
				? null
				: `${WEB_ORIGIN}${pageUrl(workspace.code, page.slug)}`,
		app: PAGE_LINK.test(app) ? app : null,
	};
};

type Exporter = typeof import('./markdownIo/exportBundle');

let exporter: Exporter | null = null;

export const loadPageExporter = (): Promise<Exporter> =>
	import('./markdownIo/exportBundle').then((module) => (exporter = module));

export const pageShareMarkdown = (
	page: Page,
	workspaceCode: string,
): string | Promise<string> => {
	const text = (module: Exporter) =>
		module.exportSinglePage(page, workspaceCode).text;
	return exporter ? text(exporter) : loadPageExporter().then(text);
};
