import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

jest.mock('@/store', () => ({
	__esModule: true,
	default: { state: { token: null }, commit: jest.fn() },
}));
it('does not silently replay a failed mutation after a server error', async () => {
	jest.useFakeTimers();
	let reject!: (error: any) => Promise<unknown>;
	const client: any = jest.fn();
	client.interceptors = {
		request: { use: jest.fn() },
		response: {
			use: (_: unknown, callback: typeof reject) => {
				reject = callback;
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
		(name: string) =>
			name === 'axios' ? { create: () => client } : require(name),
		module,
		module.exports,
	);
	const error = {
		config: { method: 'post', url: 'tasks', retry: 0 },
		response: { status: 500 },
	};
	const result = reject(error).catch((value) => value);
	await jest.runAllTimersAsync();
	expect(await result).toBe(error);
	expect(client).not.toHaveBeenCalled();
	jest.useRealTimers();
});
