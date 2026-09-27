export interface DevWatchBaseline {
	/** Fingerprints the watcher has settled on; a folder here reflects what is currently running. */
	stable: Record<string, string>;
	/** A fingerprint seen once, not yet confirmed unchanged on a second poll. */
	pending: Record<string, string>;
}

/** `null` means "no baseline yet": the next poll only seeds one, restarting nothing. Pass `null` again
 * to force a reseed, e.g. after the manual "Reload plugins" button or a developer-mode toggle. */
export type DevWatchState = DevWatchBaseline | null;

/**
 * One poll's fingerprints in, the folders to restart out. A changed fingerprint must be seen twice in a
 * row (settled) before it counts, so a plugin mid-save is not restarted half-written. A folder that
 * disappears is dropped from the baseline but never reported as "changed": removing a plugin is the
 * job of the existing full "Reload plugins" button, not of this per-file watcher.
 */
export const stepDevWatch = (
	state: DevWatchState,
	fingerprints: Record<string, string>,
): { next: DevWatchBaseline; changed: string[] } => {
	if (!state) return { next: { stable: { ...fingerprints }, pending: {} }, changed: [] };

	const stable = { ...state.stable };
	const pending = { ...state.pending };
	const changed: string[] = [];
	for (const [folder, fingerprint] of Object.entries(fingerprints)) {
		if (stable[folder] === fingerprint) {
			delete pending[folder];
			continue;
		}
		if (pending[folder] === fingerprint) {
			stable[folder] = fingerprint;
			delete pending[folder];
			changed.push(folder);
		} else {
			pending[folder] = fingerprint;
		}
	}
	for (const folder of Object.keys(stable)) {
		if (!Object.prototype.hasOwnProperty.call(fingerprints, folder)) {
			delete stable[folder];
			delete pending[folder];
		}
	}
	return { next: { stable, pending }, changed };
};
