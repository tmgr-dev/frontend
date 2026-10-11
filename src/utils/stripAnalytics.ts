const BLOCK =
	/[ \t]*<!-- analytics:start -->[\s\S]*?<!-- analytics:end -->[ \t]*\n?/g;

export const stripAnalytics = (html: string): string => html.replace(BLOCK, '');
