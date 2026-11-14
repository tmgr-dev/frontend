import {
	reachesOnlyPictures,
	reachesThisComputer,
	type WorkspacePluginRecord,
} from './cloud';
import type { PluginPackage } from './host';
import { bundleToPackage, type Release } from './market';

const WEB_UNSUPPORTED_PERMISSIONS = new Set([
	'alarms',
	'menus:task',
	'routines:read',
	'routines:write',
	'views:badge',
	'images:remote',
]);

export const webRefusal = (pkg: PluginPackage): string | null => {
	if (reachesOnlyPictures(pkg.manifest))
		return 'it loads pictures from other websites, which the web app does not allow, so it runs in the desktop app only';
	if (reachesThisComputer(pkg.manifest))
		return 'it needs this computer (files, local services, the menu bar or links), so it runs in the desktop app only';
	const unsupported = pkg.manifest.permissions.find((permission) =>
		WEB_UNSUPPORTED_PERMISSIONS.has(permission),
	);
	if (unsupported)
		return `it needs the ${unsupported} permission, which the web app does not provide`;
	return null;
};

const REFUSAL_PREFIX = 'Shared workspace plugin: ';

export const newRefusalNotices = (
	workspaceId: number,
	errors: Record<string, string>,
	seen: Set<string>,
): string[] =>
	Object.entries(errors).flatMap(([pluginId, error]) => {
		const reason = error.startsWith(REFUSAL_PREFIX)
			? error.slice(REFUSAL_PREFIX.length)
			: error;
		const key = `${workspaceId}|${pluginId}|${reason}`;
		if (seen.has(key)) return [];
		seen.add(key);
		return [`${pluginId}: ${reason}`];
	});

export const sha256Hex = async (bytes: ArrayBuffer): Promise<string> =>
	Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
		.map((byte) => byte.toString(16).padStart(2, '0'))
		.join('');

export const withoutHtmlPages = <T extends PluginPackage>(pkg: T): T => ({
	...pkg,
	pages: {},
});

/** The bundle must be byte-for-byte what the creator pinned; the proxy that delivered it is not trusted. */
export const packageFromPin = async (
	record: WorkspacePluginRecord,
	download: () => Promise<ArrayBuffer>,
): Promise<PluginPackage> => {
	const bytes = await download();
	if ((await sha256Hex(bytes)) !== record.sha256)
		throw new Error('the release is not the one this workspace pinned');
	const release: Release = {
		repo: record.repo,
		tag: record.version,
		sha256: String(record.sha256),
		bundle: new TextDecoder('utf-8', { fatal: true }).decode(bytes),
		signature: '',
		public_key: String(record.public_key),
		verified: false,
	};
	const pkg = withoutHtmlPages(bundleToPackage(release));
	if (pkg.manifest.id !== record.plugin_id)
		throw new Error('the release is a different plugin than the pinned one');
	const refusal = webRefusal(pkg);
	if (refusal) throw new Error(refusal);
	return pkg;
};
