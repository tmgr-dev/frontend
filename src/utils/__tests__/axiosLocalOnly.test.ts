import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

const store = {
	state: { token: null as any, workspaces: [] },
	getters: { currentWorkspaceId: null, isLocalOnly: true } as Record<string, any>,
	commit: jest.fn(),
	dispatch: jest.fn(),
};

jest.mock('@/store', () => ({ __esModule: true, default: store }));

const loadErrorHandler = (): ((error: any) => Promise<unknown>) => {
	let handler!: (error: any) => Promise<unknown>;
	const client: any = jest.fn();
	client.interceptors = {
		request: { use: jest.fn() },
		response: {
			use: (_ok: unknown, onError: typeof handler) => {
				handler = onError;
			},
		},
	};
	const source = fs
		.readFileSync(path.join(__dirname, '../../plugins/axios.ts'), 'utf8')
		.replaceAll('import.meta.env.VITE_API_BASE_URL', "'/api/'");
	const code = ts.transpileModule(source, {
		compilerOptions: { module: ts.ModuleKind.CommonJS, esModuleInterop: true },
	}).outputText;
	const module = { exports: {} };
	new Function('require', 'module', 'exports', code)(
		(name: string) => (name === 'axios' ? { create: () => client, post: jest.fn() } : require(name)),
		module,
		module.exports,
	);
	return handler;
};

describe('401 without an account', () => {
	beforeEach(() => {
		store.dispatch.mockReset();
		store.commit.mockReset();
	});

	it('never logs out or leaves the local workspace', async () => {
		const handler = loadErrorHandler();
		const error = { config: { headers: {}, url: 'tasks/current' }, response: { status: 401 } };

		await expect(handler(error)).rejects.toBe(error);

		expect(store.dispatch).not.toHaveBeenCalled();
	});
});
