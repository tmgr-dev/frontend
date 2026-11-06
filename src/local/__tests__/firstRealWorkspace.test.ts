import axios from 'axios';

import type { LocalWorkspace } from '../types';

const DEMO: LocalWorkspace = {
	id: -7,
	name: 'Demo: Launch week',
	code: 'demo',
	schema_version: 12,
	created_at: '',
	path: '/tmp/demo',
	database: '/tmp/demo/workspace.db',
	demo: true,
};

const REAL: LocalWorkspace = {
	id: -1,
	name: 'My workspace',
	code: 'personal',
	schema_version: 12,
	created_at: '',
	path: '/tmp/personal',
	database: '/tmp/personal/workspace.db',
};

const storage = new Map<string, string>();
(globalThis as any).localStorage = {
	getItem: (key: string) => storage.get(key) ?? null,
	setItem: (key: string, value: string) => storage.set(key, value),
	removeItem: (key: string) => storage.delete(key),
	clear: () => storage.clear(),
};

jest.mock('../runtime', () => ({
	...jest.requireActual('../runtime'),
	listLocalWorkspaces: jest.fn(),
	activeLocalWorkspace: jest.fn(),
	hasActiveLocalWorkspace: jest.fn(),
	setActiveLocalWorkspace: jest.fn(),
	localWorkspaceById: jest.fn(),
	localContext: jest.fn(),
	createLocalWorkspace: jest.fn(),
}));

// eslint-disable-next-line import/first
import * as runtime from '../runtime';
// eslint-disable-next-line import/first
import { installLocalWorkspaces } from '../install';

const { firstRealWorkspace } = jest.requireActual('../runtime');

describe('firstRealWorkspace', () => {
	it('skips demo entries', () => {
		expect(firstRealWorkspace([DEMO, REAL])).toBe(REAL);
	});

	it('returns undefined when only demo entries remain', () => {
		expect(firstRealWorkspace([DEMO])).toBeUndefined();
	});

	it('returns the first entry when none is a demo', () => {
		expect(firstRealWorkspace([REAL, { ...REAL, id: -2, code: 'work' }])).toBe(
			REAL,
		);
	});
});

describe('installLocalWorkspaces activates a real workspace over a demo', () => {
	const client = axios.create({
		adapter: async (config) => ({
			data: JSON.stringify({ data: [] }),
			status: 200,
			statusText: 'OK',
			headers: {},
			config,
		}),
	});
	let active: LocalWorkspace | null;

	installLocalWorkspaces(client, {
		currentUser: () => null,
		hasSession: () => false,
		accountless: () => true,
	});

	beforeEach(() => {
		active = null;
		jest.mocked(runtime.activeLocalWorkspace).mockImplementation(() => active);
		jest
			.mocked(runtime.hasActiveLocalWorkspace)
			.mockImplementation(() => active !== null);
		jest.mocked(runtime.setActiveLocalWorkspace).mockImplementation((w) => {
			active = w;
		});
		jest.mocked(runtime.createLocalWorkspace).mockReset();
	});

	it('opens the first real workspace when the list starts with the demo', async () => {
		jest.mocked(runtime.listLocalWorkspaces).mockResolvedValue([DEMO, REAL]);
		await client.get('workspaces');
		expect(runtime.setActiveLocalWorkspace).toHaveBeenCalledWith(REAL);
		expect(active).toEqual(REAL);
	});

	it('creates My workspace when only the demo exists', async () => {
		jest.mocked(runtime.listLocalWorkspaces).mockResolvedValue([DEMO]);
		jest.mocked(runtime.createLocalWorkspace).mockResolvedValue(REAL);
		await client.get('workspaces');
		expect(runtime.createLocalWorkspace).toHaveBeenCalledWith('My workspace');
		expect(active).toEqual(REAL);
	});
});
