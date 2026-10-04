import { pluginHost, pluginState } from './state';
import type { TaskMenuItem } from './taskMenu';

/** Reads every reactive source the item list depends on, so a computed/watch re-runs when plugins start, stop or re-register. */
export const currentTaskMenuItems = (): TaskMenuItem[] => {
	void pluginState.revision;
	void Object.values(pluginState.revisions);
	void Object.values(pluginState.plugins).map((plugin) => plugin.status);
	return pluginHost()?.taskMenuItems() ?? [];
};
