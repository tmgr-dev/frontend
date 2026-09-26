import type { PluginPackage } from './host';
import { parseManifest } from './manifest';

export interface FolderPlugin {
	folder: string;
	manifest: string;
	code: string;
}

/**
 * Folder plugins pick their own id, so an id already in use (built-in or another folder) is refused:
 * otherwise one plugin's code could run under another's permissions card and storage.
 */
export const folderPackagesFrom = (
	found: FolderPlugin[],
	reservedIds: Iterable<string>,
): { packages: PluginPackage[]; errors: Record<string, string> } => {
	const taken = new Set(reservedIds);
	const packages: PluginPackage[] = [];
	const errors: Record<string, string> = {};
	for (const { folder, manifest, code } of found) {
		try {
			const parsed = parseManifest(JSON.parse(manifest));
			if (taken.has(parsed.id))
				throw new Error(`id ${parsed.id} is already used by another plugin`);
			taken.add(parsed.id);
			packages.push({ manifest: parsed, code, source: 'folder' });
		} catch (error) {
			errors[folder] = (
				error instanceof Error ? error.message : String(error)
			).slice(0, 300);
		}
	}
	return { packages, errors };
};
