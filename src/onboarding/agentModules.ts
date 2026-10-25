import type { ModulesPayload } from '@/utils/modules';

export const AGENT_MODULES = ['agent_work', 'personas'];

export interface AgentModulesDeps {
	load(): Promise<ModulesPayload | null>;
	save(modules: Record<string, boolean>): Promise<ModulesPayload>;
}

export const agentModulesToEnable = (
	payload: ModulesPayload | null,
): Record<string, boolean> | null => {
	if (!payload || payload.configured || !payload.canManage) return null;
	const off = AGENT_MODULES.filter(
		(key) =>
			payload.modules.find((entry) => entry.key === key)?.enabled === false,
	);
	return off.length ? Object.fromEntries(off.map((key) => [key, true])) : null;
};

export async function enableAgentModules(
	intent: string,
	deps: AgentModulesDeps,
): Promise<ModulesPayload | null> {
	if (intent !== 'agent') return null;
	try {
		const modules = agentModulesToEnable(await deps.load());
		return modules ? await deps.save(modules) : null;
	} catch (error) {
		console.error('Failed to enable agent modules:', error);
		return null;
	}
}
