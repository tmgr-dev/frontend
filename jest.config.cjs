/** @type {import('ts-jest').JestConfigWithTsJest} */
// CommonJS config (.cjs) because package.json sets "type": "module";
// a plain jest.config.js with module.exports fails to load under ESM.
module.exports = {
	preset: 'ts-jest',
	testEnvironment: 'node',
	// TS-only test runner. Vue SFC component tests are intentionally not run
	// here (see TM-8825): the only such test was a no-op vue-cli scaffold stub
	// and was removed rather than wiring up vue-jest/babel for zero assertions.
	testMatch: ['**/__tests__/**/*.test.ts', '**/?(*.)+(spec|test).ts'],
	moduleNameMapper: {
		'^@/(.*)$': '<rootDir>/src/$1',
	},
	// marked ships ESM only and jest runs CommonJS here, so it is compiled like
	// the rest of the sources instead of being skipped as a node_modules file.
	transformIgnorePatterns: ['/node_modules/(?!marked/)'],
	transform: {
		'^.+\\.m?js$': [
			'ts-jest',
			{
				tsconfig: {
					allowJs: true,
					target: 'ES2020',
					module: 'CommonJS',
					esModuleInterop: true,
				},
			},
		],
		'^.+\\.tsx?$': [
			'ts-jest',
			{
				tsconfig: {
					target: 'ES2020',
					module: 'CommonJS',
					lib: ['ES2021', 'DOM'],
					esModuleInterop: true,
					baseUrl: '.',
					paths: { '@/*': ['./src/*'] },
				},
			},
		],
	},
};
