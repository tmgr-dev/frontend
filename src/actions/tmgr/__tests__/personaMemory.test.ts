jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: jest.fn(),
		post: jest.fn(),
		put: jest.fn(),
		patch: jest.fn(),
		delete: jest.fn(),
	},
}));

import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	addPersonaMemory,
	forgetPersonaMemory,
	getSelfMemory,
	getWorkspaceMemoryOverview,
	listDeletedPersonaMemory,
	listPersonaMemory,
	memoryErrorMessage,
	putSelfMemory,
	restorePersonaMemory,
	updatePersonaMemory,
} from '../personaMemory';

const ENTRY = { id: 1, topic: 'notes', body: 'x' };

beforeEach(() => {
	requestCache.clear();
	jest.clearAllMocks();
});

describe('persona memory actions', () => {
	it('lists entries with workspace_id and filters, dropping empty ones', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [ENTRY] } });
		const rows = await listPersonaMemory('u1', 5, {
			topic: 'notes',
			query: '',
			journal: true,
			limit: 50,
		});
		expect(rows).toEqual([ENTRY]);
		expect(axios.get).toHaveBeenCalledWith('/personas/u1/memory', {
			params: { workspace_id: 5, topic: 'notes', journal: true, limit: 50 },
		});
	});

	it('adds, updates, forgets and restores with workspace_id', async () => {
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: ENTRY } });
		(axios.patch as jest.Mock).mockResolvedValue({ data: { data: ENTRY } });
		(axios.delete as jest.Mock).mockResolvedValue({});

		await addPersonaMemory('u1', 5, { topic: 'notes', body: 'x' });
		expect(axios.post).toHaveBeenCalledWith(
			'/personas/u1/memory',
			{ topic: 'notes', body: 'x' },
			{ params: { workspace_id: 5 } },
		);

		await updatePersonaMemory('u1', 5, 1, { body: 'y' });
		expect(axios.patch).toHaveBeenCalledWith(
			'/personas/u1/memory/1',
			{ body: 'y' },
			{ params: { workspace_id: 5 } },
		);

		await forgetPersonaMemory('u1', 5, 1);
		expect(axios.delete).toHaveBeenCalledWith('/personas/u1/memory/1', {
			params: { workspace_id: 5 },
		});

		await restorePersonaMemory('u1', 5, 1);
		expect(axios.post).toHaveBeenCalledWith(
			'/personas/u1/memory/1/restore',
			undefined,
			{ params: { workspace_id: 5 } },
		);
	});

	it('lists deleted entries', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [ENTRY] } });
		await listDeletedPersonaMemory('u1', 5);
		expect(axios.get).toHaveBeenCalledWith('/personas/u1/memory/deleted', {
			params: { workspace_id: 5 },
		});
	});

	it('reads and writes self memory', async () => {
		(axios.get as jest.Mock).mockResolvedValue({
			data: { data: { body: 'a' } },
		});
		expect(await getSelfMemory('u1')).toBe('a');
		(axios.put as jest.Mock).mockResolvedValue({
			data: { data: { body: 'b' } },
		});
		expect(await putSelfMemory('u1', 'b')).toBe('b');
		expect(axios.put).toHaveBeenCalledWith('/personas/u1/memory/self', {
			body: 'b',
		});
	});

	it('caches the workspace overview until a mutation', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [] } });
		await getWorkspaceMemoryOverview(5);
		await getWorkspaceMemoryOverview(5);
		expect(axios.get).toHaveBeenCalledTimes(1);
		expect(axios.get).toHaveBeenCalledWith('/workspaces/5/persona-memory');
		(axios.delete as jest.Mock).mockResolvedValue({});
		await forgetPersonaMemory('u1', 5, 1);
		await getWorkspaceMemoryOverview(5);
		expect(axios.get).toHaveBeenCalledTimes(2);
	});
});

describe('memoryErrorMessage', () => {
	const err = (status: number, data: object) => ({
		response: { status, data },
	});

	it('maps a detected secret to a fixed message', () => {
		expect(
			memoryErrorMessage(err(422, { error: 'memory_secret_detected' })),
		).toBe('Looks like a secret — not saved');
	});

	it('shows the server message otherwise', () => {
		expect(memoryErrorMessage(err(422, { message: 'Memory is full' }))).toBe(
			'Memory is full',
		);
	});

	it('falls back to a generic message', () => {
		expect(memoryErrorMessage(new Error('x'))).toBe('Something went wrong');
	});
});
