const BLOCK =
	/[ \t]*<!-- analytics:start -->[\s\S]*?<!-- analytics:end -->[ \t]*\n?/g;

export const stripAnalytics = (html: string): string => html.replace(BLOCK, '');

export const AUTH_PATH =
	/^\/(login|register|desktop-auth|password|email\/verify|auth\/link-confirm|workspaces\/invitations)(\/|$)/i;

export const isAuthPath = (pathname: string): boolean =>
	AUTH_PATH.test(pathname);

export const carriesSecret = (fullPath: string): boolean => {
	const path = fullPath.split(/[?#]/)[0];
	return (
		isAuthPath(path) &&
		(/[?#]/.test(fullPath) || /^\/workspaces\/invitations\/[^/]/i.test(path))
	);
};
