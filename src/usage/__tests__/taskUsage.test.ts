import { readFileSync } from 'fs';
import { join } from 'path';

const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...a: unknown[]) => invoke(...a),
}));

import { usageTask } from '../taskUsage';

describe('usageTask', () => {
	beforeEach(() => invoke.mockReset());

	it('calls usage_task with the camel case task id', async () => {
		invoke.mockResolvedValue({ supported: true });
		const runs = [{ id: 1 }] as any;
		await usageTask({ runs, workspace: 'ws', task_id: 7 });
		expect(invoke).toHaveBeenCalledWith('usage_task', {
			runs,
			workspace: 'ws',
			taskId: 7,
		});
	});

	it('is allowed for the main window in the command manifest and capability', () => {
		const root = join(__dirname, '../../../src-tauri');
		expect(readFileSync(join(root, 'build.rs'), 'utf8')).toContain(
			'"usage_task",',
		);
		const capability = JSON.parse(
			readFileSync(join(root, 'capabilities/usage.json'), 'utf8'),
		);
		expect(capability.windows).toEqual(['main', 'task-*']);
		expect(capability.permissions).toContain('allow-usage-task');
	});
});
