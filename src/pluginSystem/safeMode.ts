type Invoke = (
	command: string,
	args?: Record<string, unknown>,
) => Promise<unknown>;

const tauriInvoke: Invoke = async (command, args) => {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke(command, args);
};

export const pushSafeMode = async (
	value: boolean,
	invoke: Invoke = tauriInvoke,
): Promise<boolean> => {
	try {
		await invoke('plugins_set_safe_mode', { value });
		return true;
	} catch (error) {
		console.error('plugins_set_safe_mode failed', error);
		return false;
	}
};

export const syncStoredSafeMode = async (
	stored: boolean,
	desktopHasIt: boolean,
	invoke: Invoke = tauriInvoke,
): Promise<void> => {
	if (stored && !desktopHasIt) await pushSafeMode(true, invoke);
};
