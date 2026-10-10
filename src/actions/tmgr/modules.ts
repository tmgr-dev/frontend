import { isLocalWorkspaceId } from '@/local/classify';
import $axios from '@/plugins/axios';
import {
	modulesToMap,
	parseModulesPayload,
	type ModuleEntry,
	type ModulesMap,
	type ModulesPayload,
} from '@/utils/modules';

export const getWorkspaceModules = async (
	workspaceId: number | string,
): Promise<ModulesPayload | null> => {
	if (isLocalWorkspaceId(workspaceId)) return null;
	try {
		const { data } = await $axios.get(`/workspaces/${workspaceId}/modules`);
		return parseModulesPayload(data);
	} catch (error: any) {
		if ([404, 501].includes(error?.response?.status)) return null;
		throw error;
	}
};

export const getLegacyWorkspaceFeatureToggles = async (
	workspaceId: number | string,
): Promise<ModulesMap> => {
	const {
		data: { data },
	} = await $axios.get(`/workspaces/${workspaceId}/feature-toggles`);
	return data;
};

export const getWorkspaceFeatureToggles = async (
	workspaceId: number | string,
): Promise<ModulesMap> => {
	const payload = await getWorkspaceModules(workspaceId);
	return payload
		? modulesToMap(payload)
		: getLegacyWorkspaceFeatureToggles(workspaceId);
};

export const setModuleEnabled = async (
	workspaceId: number | string,
	entry: Pick<ModuleEntry, 'key' | 'scope'>,
	enabled: boolean,
): Promise<ModulesPayload | null> => {
	const url =
		entry.scope === 'user'
			? '/user/feature-toggles'
			: `/workspaces/${workspaceId}/feature-toggles`;
	await $axios.put(url, { features: { [entry.key]: enabled } });
	return getWorkspaceModules(workspaceId);
};

export const hideModuleForMe = async (
	workspaceId: number | string,
	key: string,
	hide: boolean,
): Promise<ModulesPayload> => {
	const { data } = await $axios.put(
		`/workspaces/${workspaceId}/modules/hidden`,
		{
			[key]: hide,
		},
	);
	return parseModulesPayload(data);
};

export const saveModulesChoice = async (
	workspaceId: number | string,
	choice: { preset?: string; modules?: Record<string, boolean> },
): Promise<ModulesPayload> => {
	const { data } = await $axios.put(
		`/workspaces/${workspaceId}/modules`,
		choice,
	);
	return parseModulesPayload(data);
};
