import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		state: { token: null, workspaces: [{ id: 5 }, { id: 6 }] },
		getters: { currentWorkspaceId: 6 },
		commit: jest.fn(),
	},
}));

const loadRequestInterceptor = (): ((config: any) => any) => {
	let handler!: (config: any) => any;
	const client: any = jest.fn();
	client.interceptors = {
		request: {
			use: (callback: typeof handler) => {
				handler = callback;
			},
		},
		response: { use: jest.fn() },
	};
	const source = fs
		.readFileSync(path.join(__dirname, '../../plugins/axios.ts'), 'utf8')
		.replaceAll('import.meta.env.VITE_API_BASE_URL', "'/api/'");
	const code = ts.transpileModule(source, {
		compilerOptions: { module: ts.ModuleKind.CommonJS, esModuleInterop: true },
	}).outputText;
	const module = { exports: {} };
	new Function('require', 'module', 'exports', code)(
		(name: string) =>
			name === 'axios' ? { create: () => client } : require(name),
		module,
		module.exports,
	);
	return handler;
};

describe('X-Workspace-Id request header', () => {
	it('attaches the current workspace by default', () => {
		const config = loadRequestInterceptor()({ headers: {} });
		expect(config.headers['X-Workspace-Id']).toBe('6');
	});

	it('keeps a header the caller set explicitly', () => {
		const config = loadRequestInterceptor()({
			headers: { 'X-Workspace-Id': '5' },
		});
		expect(config.headers['X-Workspace-Id']).toBe('5');
	});
});
