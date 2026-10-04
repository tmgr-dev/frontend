import { TASK_MENU_LOCATION, type PluginManifest } from './manifest';

export interface TaskMenuItem {
	pluginId: string;
	pluginName: string;
	command: string;
	title: string;
}

export const TASK_MENU_SUBMENU_THRESHOLD = 4;

/** Items of running plugins with menus:task whose command is declared and registered, in plugin order. */
export const resolveTaskMenuItems = (
	plugins: {
		manifest: PluginManifest;
		running: boolean;
		registered: Set<string>;
	}[],
): TaskMenuItem[] =>
	plugins.flatMap(({ manifest, running, registered }) =>
		running && manifest.permissions.includes('menus:task')
			? manifest.contributes.menus[TASK_MENU_LOCATION].filter((item) =>
					registered.has(item.command),
			  ).map((item) => ({
					pluginId: manifest.id,
					pluginName: manifest.name,
					command: item.command,
					title: item.title,
			  }))
			: [],
	);

export const needsTaskMenuSubmenu = (items: TaskMenuItem[]) =>
	items.length > TASK_MENU_SUBMENU_THRESHOLD;
