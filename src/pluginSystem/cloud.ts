import { reactive } from 'vue';
import type { PluginWorkspace } from './broker';
import type { PluginEntry } from './host';
import type { PluginManifest } from './manifest';
import type { Release } from './market';
import { machineConsentStore } from './storage';

/** A plugin the creator of a shared workspace turned on for everyone, pinned to one release. */
export interface WorkspacePluginRecord {
	plugin_id: string;
	repo: string;
	version: string;
	sha256: string | null;
	public_key: string | null;
	permissions: string[];
	enabled_by: number;
}

export const BUILTIN_REPO = 'builtin';

/** By shared workspace id, as last read from the server. */
export const workspacePlugins = reactive<Record<number, WorkspacePluginRecord[]>>(
	{},
);

export const recordFor = (workspaceId: number, pluginId: string) =>
	workspacePlugins[workspaceId]?.find((r) => r.plugin_id === pluginId) ?? null;

/** Runs only exactly what was pinned: that built-in, or the installed release with the pinned checksum. */
export const matchesPin = (
	record: WorkspacePluginRecord,
	entry: Pick<PluginEntry, 'source' | 'origin'> | undefined,
) =>
	!!entry &&
	(record.repo === BUILTIN_REPO
		? entry.source === 'builtin'
		: entry.source === 'installed' &&
		  entry.origin?.repo === record.repo &&
		  entry.origin?.sha256 === record.sha256);

/** Why a downloaded release is not the one the workspace pinned. */
export const pinMismatch = (record: WorkspacePluginRecord, release: Release) =>
	release.sha256 !== record.sha256
		? 'the release on GitHub is not the one this workspace pinned'
		: release.public_key !== record.public_key
		? 'the release is signed with a different key than the pinned one'
		: null;

const MACHINE_PERMISSIONS = new Set([
	'files:export',
	'files:pick',
	'tray',
	'deeplinks',
	'links:open',
]);

/** Files, local services, the menu bar, tmgr:// links or the browser of the member's own computer. */
export const reachesThisComputer = (manifest: PluginManifest) =>
	manifest.network.allowedOrigins.length > 0 ||
	manifest.permissions.some((p) => MACHINE_PERMISSIONS.has(p));

/** Local-only permissions the server's EnablePluginRequest does not know and rejects with 400. */
const CLOUD_UNSUPPORTED_PERMISSIONS = new Set(['routines:read', 'routines:write']);

const cloudPermissionsOf = (manifest: PluginManifest) =>
	manifest.permissions.filter((p) => !CLOUD_UNSUPPORTED_PERMISSIONS.has(p));

/** What the server stores when the creator turns a plugin on; folder plugins cannot be shared. */
export const pinOf = (entry: PluginEntry) => {
	if (entry.source === 'builtin')
		return {
			repo: BUILTIN_REPO,
			version: entry.manifest.version,
			sha256: null,
			public_key: null,
			permissions: cloudPermissionsOf(entry.manifest),
		};
	if (entry.source === 'installed' && entry.origin?.public_key)
		return {
			repo: entry.origin.repo,
			version: entry.origin.tag,
			sha256: entry.origin.sha256,
			public_key: entry.origin.public_key,
			permissions: cloudPermissionsOf(entry.manifest),
		};
	return null;
};

/** Consent follows the pinned release: a new pin asks again. */
export const releaseOf = (record: WorkspacePluginRecord) =>
	record.repo === BUILTIN_REPO ? 'builtin' : String(record.sha256);

export const hasMachineConsent = (
	workspace: PluginWorkspace | null,
	pluginId: string,
	memberId: number,
) => {
	if (!workspace) return false;
	if (workspace.kind === 'local') return true;
	const record = recordFor(workspace.id, pluginId);
	return (
		!!record &&
		memberId > 0 &&
		machineConsentStore.has(memberId, workspace.id, pluginId, releaseOf(record))
	);
};
