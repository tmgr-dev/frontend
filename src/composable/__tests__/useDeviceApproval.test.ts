import { createDeviceApproval } from '../useDeviceApproval';

const request = {
	client_name: 'tmgr-cli',
	host_label: 'work-laptop',
	requested_scopes: ['tasks:read', 'tasks:write'],
	created_at: '2026-10-12T10:00:00Z',
	expires_at: '2026-10-12T10:10:00Z',
};

const http = (status: number) => ({ response: { status } });

const make = (overrides = {}) => {
	const deps = {
		lookup: jest.fn().mockResolvedValue(request),
		approve: jest.fn().mockResolvedValue(undefined),
		deny: jest.fn().mockResolvedValue(undefined),
		...overrides,
	};
	return { deps, approval: createDeviceApproval(deps) };
};

describe('createDeviceApproval', () => {
	it('does not look up an incomplete code', async () => {
		const { deps, approval } = make();
		approval.setCode('abcd');
		expect(approval.canLookup.value).toBe(false);
		await approval.find();
		expect(deps.lookup).not.toHaveBeenCalled();
		expect(approval.state.value).toBe('entry');
	});

	it('looks up a normalised code and preselects every requested scope', async () => {
		const { deps, approval } = make();
		approval.setCode('abcd2345');
		await approval.find();
		expect(deps.lookup).toHaveBeenCalledWith('ABCD-2345');
		expect(approval.state.value).toBe('ready');
		expect(approval.selected.value).toEqual(['tasks:read', 'tasks:write']);
	});

	it('approves only the selected scopes', async () => {
		const { deps, approval } = make();
		approval.setCode('ABCD-2345');
		await approval.find();
		approval.toggle('tasks:write');
		await approval.approve();
		expect(deps.approve).toHaveBeenCalledWith('ABCD-2345', ['tasks:read']);
		expect(approval.state.value).toBe('approved');
	});

	it('refuses to approve with no scopes selected', async () => {
		const { deps, approval } = make();
		approval.setCode('ABCD-2345');
		await approval.find();
		approval.toggle('tasks:read');
		approval.toggle('tasks:write');
		expect(approval.canApprove.value).toBe(false);
		await approval.approve();
		expect(deps.approve).not.toHaveBeenCalled();
		expect(approval.state.value).toBe('ready');
	});

	it('denies', async () => {
		const { deps, approval } = make();
		approval.setCode('ABCD-2345');
		await approval.find();
		await approval.deny();
		expect(deps.deny).toHaveBeenCalledWith('ABCD-2345');
		expect(approval.state.value).toBe('denied');
	});

	it.each([
		[404, 'not_found'],
		[410, 'not_found'],
		[429, 'locked'],
		[500, 'error'],
	])('maps lookup status %i to %s', async (status, expected) => {
		const { approval } = make({
			lookup: jest.fn().mockRejectedValue(http(status)),
		});
		approval.setCode('ABCD-2345');
		await approval.find();
		expect(approval.state.value).toBe(expected);
	});

	it('maps a vanished request on approve to not_found and an outage to error', async () => {
		const gone = make({ approve: jest.fn().mockRejectedValue(http(404)) });
		gone.approval.setCode('ABCD-2345');
		await gone.approval.find();
		await gone.approval.approve();
		expect(gone.approval.state.value).toBe('not_found');

		const down = make({ approve: jest.fn().mockRejectedValue(http(500)) });
		down.approval.setCode('ABCD-2345');
		await down.approval.find();
		await down.approval.approve();
		expect(down.approval.state.value).toBe('error');
		down.approval.retry();
		expect(down.approval.state.value).toBe('ready');
	});

	it('reset returns to an empty entry form', async () => {
		const { approval } = make({
			lookup: jest.fn().mockRejectedValue(http(404)),
		});
		approval.setCode('ABCD-2345');
		await approval.find();
		approval.reset();
		expect(approval.state.value).toBe('entry');
		expect(approval.code.value).toBe('');
	});
});
