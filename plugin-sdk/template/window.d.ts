/**
 * Types for a plugin's own page (a view with `ui` in the manifest). The page opens in a separate window,
 * served from tmgrplugin:// with a strict CSP: inline scripts and styles only, no network, no forms.
 * Its one way to the app is `window.tmgr`, answered by the plugin's broker with the plugin's permissions.
 */
declare const tmgr: {
	/**
	 * Any broker method the plugin may call from its logic, except register and log. If this window was
	 * opened from a `tmgr://plugin/<id>/view/<id>?...` link, `call('deepLink.params')` returns the link's
	 * query params as a flat string map once, then null on later calls.
	 */
	call<T = unknown>(method: string, params?: unknown): Promise<T>;
	/** Runs one of the plugin's own declared commands in its sandbox. */
	runCommand<T = unknown>(id: string, args?: unknown): Promise<T>;
	tasks: {
		list(query?: { statusId?: number; categoryId?: number; search?: string; page?: number; perPage?: number }): Promise<{
			items: Record<string, unknown>[];
			total: number;
		}>;
		get(id: number): Promise<Record<string, unknown>>;
		update(id: number, patch: Record<string, unknown>): Promise<Record<string, unknown>>;
	};
	statuses: { list(): Promise<{ id: number; name: string; type: string }[]> };
	categories: { list(): Promise<{ id: number; title: string; code: string | null }[]> };
	storage: { get<T = unknown>(key: string): Promise<T | null>; set(key: string, value: unknown): Promise<void> };
	settings: { get(): Promise<Record<string, unknown>> };
	notify(message: string): Promise<void>;
};
