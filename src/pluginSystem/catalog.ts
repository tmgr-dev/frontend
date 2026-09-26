import { reactive } from 'vue';

export interface CatalogEntry {
	id: string;
	repo: string;
	public_key: string;
	name?: string | null;
	description?: string | null;
}

export interface CatalogBlock {
	id?: string | null;
	repo?: string | null;
	sha256?: string | null;
	reason: string;
}

/** Verified publishers and the blocklist; Rust verified the TMGR signature before handing it over. */
export interface Catalog {
	serial: number;
	plugins: CatalogEntry[];
	blocked: CatalogBlock[];
}

export const pluginCatalog = reactive<Catalog>({
	serial: 0,
	plugins: [],
	blocked: [],
});

export const REFRESH_MS = 6 * 60 * 60 * 1000;

/** Built-in and folder plugins have no repository or release, so only their id can be blocked. */
export const blockedById = (catalog: Catalog, pluginId: string) =>
	catalog.blocked.find((block) => block.id === pluginId)?.reason.slice(0, 200) ??
	null;

export const setCatalog = (next: Catalog) => {
	if (next.serial < pluginCatalog.serial) return;
	pluginCatalog.serial = next.serial;
	pluginCatalog.plugins = next.plugins ?? [];
	pluginCatalog.blocked = next.blocked ?? [];
};
