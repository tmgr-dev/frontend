import type { PluginPackage } from './host';
import { parseManifest, type PluginManifest } from './manifest';

export interface Release {
	repo: string;
	tag: string;
	sha256: string;
	bundle: string;
	signature: string;
	public_key: string;
	/** The TMGR catalog vouches for the publisher key; otherwise the key was pinned on first install. */
	verified: boolean;
	/** Set by Rust when listing installed plugins that the blocklist names. */
	blocked?: string | null;
}

const PAGE = /^ui\/[a-z0-9][a-z0-9_-]*\.html$/i;
const MAX_PAGES = 10;
const MAX_PAGE_BYTES = 1024 * 1024;

/** A release's tmgr-plugin.json: `{ manifest, code, pages }`, checked like any other plugin before use. */
export const bundleToPackage = (
	release: Release,
): PluginPackage & { origin: NonNullable<PluginPackage['origin']> } => {
	let raw: any;
	try {
		raw = JSON.parse(release.bundle);
	} catch {
		throw new Error('the plugin bundle is not valid JSON');
	}
	const manifest = parseManifest(raw?.manifest);
	if (typeof raw.code !== 'string')
		throw new Error('the plugin bundle has no code');
	const pages: Record<string, string> = {};
	const entries = Object.entries(
		raw.pages && typeof raw.pages === 'object' ? raw.pages : {},
	);
	if (entries.length > MAX_PAGES)
		throw new Error(`a plugin may bring at most ${MAX_PAGES} pages`);
	for (const [path, html] of entries) {
		if (
			!PAGE.test(path) ||
			typeof html !== 'string' ||
			new TextEncoder().encode(html).length > MAX_PAGE_BYTES
		) {
			throw new Error(
				`page ${path} must be a ui/<name>.html file of at most 1 MB`,
			);
		}
		pages[path] = html;
	}
	const missing = manifest.contributes.views.find(
		(view) => view.ui && !(view.ui in pages),
	);
	if (missing)
		throw new Error(
			`view ${missing.id} needs ${missing.ui}, which is not in the bundle`,
		);
	return {
		manifest,
		code: raw.code,
		pages,
		source: 'installed',
		origin: {
			repo: release.repo,
			tag: release.tag,
			sha256: release.sha256,
			verified: release.verified,
		},
	};
};

/** What an update would newly allow, so the user agrees to exactly that. */
export const permissionChanges = (
	before: PluginManifest,
	after: PluginManifest,
) => ({
	permissions: after.permissions.filter((p) => !before.permissions.includes(p)),
	origins: after.network.allowedOrigins.filter(
		(o) => !before.network.allowedOrigins.includes(o),
	),
});
