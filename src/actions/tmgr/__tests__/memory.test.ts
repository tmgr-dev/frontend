jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: jest.fn(),
		post: jest.fn(),
		put: jest.fn(),
		delete: jest.fn(),
	},
}));

import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	acceptMemoryProposal,
	addMemoryEntry,
	deleteMemoryEntry,
	getMemoryProposals,
	getWorkspaceMemory,
	rejectMemoryProposal,
	restoreMemoryEntry,
	undoAddedToMemory,
	updateMemoryProposal,
} from '../memory';

const ax = $axios as any;
const ok = (data: any) => Promise.resolve({ data: { data } });
const fail = (status: number, body: any = {}) =>
	Promise.reject({ response: { status, data: body } });

beforeEach(() => {
	jest.clearAllMocks();
	requestCache.invalidate(/.*/);
});

test('reads memory and proposals from the workspace routes', async () => {
	ax.get.mockImplementation((url: string) =>
		ok(url.endsWith('/proposals') ? [{ id: 5 }] : { topics: [] }),
	);
	expect(await getWorkspaceMemory(7)).toEqual({ topics: [] });
	expect(ax.get).toHaveBeenCalledWith('/workspaces/7/memory');
	expect(await getMemoryProposals(7)).toEqual([{ id: 5 }]);
	expect(ax.get).toHaveBeenCalledWith('/workspaces/7/memory/proposals');
});

test('a mutation drops cached reads of that workspace only', async () => {
	ax.get.mockImplementation(() => ok({ topics: [] }));
	ax.post.mockImplementation(() => ok({ id: 'x' }));
	await getWorkspaceMemory(7, true);
	await getWorkspaceMemory(8, true);
	await getWorkspaceMemory(7, true);
	expect(ax.get).toHaveBeenCalledTimes(2);
	await addMemoryEntry(7, 'decisions', 'We ship backend first');
	expect(ax.post).toHaveBeenCalledWith('/workspaces/7/memory', {
		topic: 'decisions',
		fact: 'We ship backend first',
	});
	await getWorkspaceMemory(7, true);
	await getWorkspaceMemory(8, true);
	expect(ax.get).toHaveBeenCalledTimes(3);
});

test('delete and restore use the entry routes with an encoded id', async () => {
	ax.delete.mockImplementation(() => ok({ deleted: true, entry: {} }));
	ax.post.mockImplementation(() => ok({ id: 'a/b' }));
	await deleteMemoryEntry(7, 'a/b');
	expect(ax.delete).toHaveBeenCalledWith('/workspaces/7/memory/entries/a%2Fb');
	await restoreMemoryEntry(7, 'a/b');
	expect(ax.post).toHaveBeenCalledWith('/workspaces/7/memory/entries/restore', {
		id: 'a/b',
	});
});

test('accept and update hit the proposal routes', async () => {
	ax.post.mockImplementation(() => ok({ id: 5, status: 'added' }));
	ax.put.mockImplementation(() => ok({ id: 5 }));
	await acceptMemoryProposal(7, 5);
	expect(ax.post).toHaveBeenCalledWith(
		'/workspaces/7/memory/proposals/5/accept',
	);
	await updateMemoryProposal(7, 5, { fact: 'new' });
	expect(ax.put).toHaveBeenCalledWith('/workspaces/7/memory/proposals/5', {
		fact: 'new',
	});
});

describe('rejectMemoryProposal', () => {
	it('returns the rejected proposal', async () => {
		ax.post.mockImplementation(() => ok({ id: 5, status: 'rejected' }));
		const result = await rejectMemoryProposal(7, 5);
		expect(result.outcome).toBe('rejected');
	});
	it('turns 409 with an added proposal into already_added', async () => {
		const proposal = { id: 5, status: 'added', entry_id: 'e1' };
		ax.post.mockImplementation(() => fail(409, { data: proposal }));
		expect(await rejectMemoryProposal(7, 5)).toEqual({
			outcome: 'already_added',
			proposal,
		});
	});
	it('rethrows any other failure', async () => {
		ax.post.mockImplementation(() => fail(409, { message: 'x' }));
		await expect(rejectMemoryProposal(7, 5)).rejects.toBeDefined();
		ax.post.mockImplementation(() => fail(500));
		await expect(rejectMemoryProposal(7, 5)).rejects.toBeDefined();
	});
});

describe('undoAddedToMemory', () => {
	it('confirms removal', async () => {
		ax.post.mockImplementation(() => ok({ id: 5, status: 'rejected' }));
		expect(await undoAddedToMemory(7, 5)).toBe('Removed from memory');
		expect(ax.post).toHaveBeenCalledWith(
			'/workspaces/7/memory/proposals/5/undo',
		);
	});
	it('reports a 409 as already changed', async () => {
		ax.post.mockImplementation(() => fail(409));
		expect(await undoAddedToMemory(7, 5)).toBe('Already changed');
	});
	it('rethrows other errors', async () => {
		ax.post.mockImplementation(() => fail(500));
		await expect(undoAddedToMemory(7, 5)).rejects.toBeDefined();
	});
});
