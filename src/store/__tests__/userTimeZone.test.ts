import * as timezone from '@/utils/timezone';
import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';

const loadStore = () => {
	const source = transpileModule(
		readFileSync(require.resolve('../index.js'), 'utf8'),
		{ compilerOptions: { module: ModuleKind.CommonJS } },
	).outputText;
	const loaded = { exports: {} as any };
	const deps: Record<string, unknown> = {
		vuex: { createStore: (options: any) => options },
		'@/utils/timezone': timezone,
	};
	new Function(
		'require',
		'module',
		'exports',
		'localStorage',
		'sessionStorage',
		source,
	)(
		(name: string) => deps[name] ?? { default: {} },
		loaded,
		loaded.exports,
		{ getItem: () => null, setItem: () => {}, removeItem: () => {} },
		{ getItem: () => null, setItem: () => {}, removeItem: () => {} },
	);
	return loaded.exports.default;
};

const stateWith = (settings: any[]) => {
	const setting = (s: any) => [s.key, s] as const;
	return {
		user: { id: 1, settings },
		userSettingsMap: Object.fromEntries(settings.map(setting)),
	};
};

describe('setUserTimeZone', () => {
	test('without a stored row only the map changes, so user.settings stays id-complete', () => {
		const { mutations, getters } = loadStore();
		const state = stateWith([{ id: 5, key: 'theme', value: 'default' }]);
		mutations.setUserTimeZone(state, 'Asia/Tokyo');
		expect(state.user.settings).toEqual([
			{ id: 5, key: 'theme', value: 'default' },
		]);
		expect(
			state.user.settings.every((s: any) => typeof s.id === 'number'),
		).toBe(true);
		expect(getters.userTimeZone(state)).toBe('Asia/Tokyo');
	});

	test('with a stored row both the row and the map are updated', () => {
		const { mutations, getters } = loadStore();
		const state = stateWith([
			{ id: 8, key: 'timezone', value: 'Europe/Warsaw' },
		]);
		mutations.setUserTimeZone(state, 'Asia/Tokyo');
		expect(state.user.settings).toEqual([
			{ id: 8, key: 'timezone', value: 'Asia/Tokyo' },
		]);
		expect(getters.userTimeZone(state)).toBe('Asia/Tokyo');
	});
});
