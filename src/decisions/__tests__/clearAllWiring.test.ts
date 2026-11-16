import { readFileSync } from 'fs';
import { join } from 'path';
import { parse } from 'vue/compiler-sfc';

const read = (name: string): string =>
	readFileSync(join(__dirname, '..', 'components', name), 'utf8');

const template = (name: string): string =>
	parse(read(name)).descriptor.template?.content ?? '';

describe('Clear all', () => {
	it('asks for confirmation before emitting clear-all', () => {
		const source = template('QueueList.vue');
		expect(source).toMatch(
			/v-if="clearCount > 0 && !confirmingClear"[\s\S]*?@click="confirmingClear = true"/,
		);
		expect(source).toMatch(
			/v-if="clearCount > 0 && confirmingClear"[\s\S]*?decisions\.queue\.clearConfirm[\s\S]*?@click="confirmClear"/,
		);
		expect(read('QueueList.vue')).toMatch(
			/confirmClear\(\) \{\s*this\.confirmingClear = false;\s*this\.\$emit\('clear-all'\);/,
		);
	});

	it('counts every pending card and calls the bulk command once', () => {
		const app = read('App.vue');
		expect(template('App.vue')).toMatch(
			/:clear-count="pendingCount"[\s\S]*?@clear-all="clearAll"/,
		);
		expect(app).toMatch(
			/pendingCount\(\): number \{\s*return this\.cards\.filter\(isPending\)\.length;/,
		);
		expect((app.match(/decisionsDismissAll\(\)/g) ?? []).length).toBe(1);
		expect(readFileSync(join(__dirname, '..', 'api.ts'), 'utf8')).toContain(
			"invoke<number>('decisions_dismiss_all')",
		);
	});
});
