import { parseModulesPayload } from '@/utils/modules';
import { agentModulesToEnable, enableAgentModules } from '../agentModules';

const payload = ({
	configured = false,
	can_manage = true,
	agent_work = false,
	personas = false,
} = {}) =>
	parseModulesPayload({
		data: {
			configured,
			can_manage,
			modules: [
				{ key: 'agent_work', enabled: agent_work },
				{ key: 'personas', enabled: personas },
				{ key: 'pages', enabled: false },
			],
		},
	});

const deps = (loaded: ReturnType<typeof payload> | null) => {
	const saved: Record<string, boolean>[] = [];
	return {
		saved,
		load: jest.fn(async () => loaded),
		save: jest.fn(async (modules: Record<string, boolean>) => {
			saved.push(modules);
			return payload({ configured: true, agent_work: true, personas: true });
		}),
	};
};

describe('agentModulesToEnable', () => {
	it('turns on agent work and personas in a fresh workspace the user manages', () => {
		expect(agentModulesToEnable(payload())).toEqual({
			agent_work: true,
			personas: true,
		});
	});

	it('sends only the modules that are still off', () => {
		expect(agentModulesToEnable(payload({ personas: true }))).toEqual({
			agent_work: true,
		});
		expect(
			agentModulesToEnable(payload({ agent_work: true, personas: true })),
		).toBeNull();
	});

	it('leaves a workspace whose modules were already chosen alone', () => {
		expect(agentModulesToEnable(payload({ configured: true }))).toBeNull();
	});

	it('does nothing for a workspace the user cannot manage', () => {
		expect(agentModulesToEnable(payload({ can_manage: false }))).toBeNull();
	});

	it('does nothing without a modules payload or the module entries', () => {
		expect(agentModulesToEnable(null)).toBeNull();
		expect(
			agentModulesToEnable(
				parseModulesPayload({
					data: { configured: false, can_manage: true, modules: [] },
				}),
			),
		).toBeNull();
	});
});

describe('enableAgentModules', () => {
	it('saves the agent modules for the agent intent', async () => {
		const d = deps(payload());
		const result = await enableAgentModules('agent', d);
		expect(d.saved).toEqual([{ agent_work: true, personas: true }]);
		expect(result?.configured).toBe(true);
	});

	it.each(['human', 'skipped'])(
		'changes nothing for the %s intent',
		async (intent) => {
			const d = deps(payload());
			expect(await enableAgentModules(intent, d)).toBeNull();
			expect(d.load).not.toHaveBeenCalled();
			expect(d.save).not.toHaveBeenCalled();
		},
	);

	it('does not save when the workspace was configured or is not manageable', async () => {
		for (const loaded of [
			payload({ configured: true }),
			payload({ can_manage: false }),
		]) {
			const d = deps(loaded);
			expect(await enableAgentModules('agent', d)).toBeNull();
			expect(d.save).not.toHaveBeenCalled();
		}
	});

	it('swallows a failed load or save so onboarding goes on', async () => {
		const error = jest.spyOn(console, 'error').mockImplementation(() => {});
		const failedLoad = deps(null);
		failedLoad.load.mockRejectedValueOnce(new Error('500'));
		expect(await enableAgentModules('agent', failedLoad)).toBeNull();
		const failedSave = deps(payload());
		failedSave.save.mockRejectedValueOnce(new Error('403'));
		expect(await enableAgentModules('agent', failedSave)).toBeNull();
		expect(error).toHaveBeenCalledTimes(2);
		error.mockRestore();
	});
});
