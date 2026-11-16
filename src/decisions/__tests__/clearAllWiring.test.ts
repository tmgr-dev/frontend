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
			/v-if="clearIds.length > 0 && !confirmingClear"[\s\S]*?@click="openConfirm"/,
		);
		expect(source).toMatch(
			/v-if="clearIds.length > 0 && confirmingClear"[\s\S]*?decisions\.queue\.clearConfirm[\s\S]*?@click="confirmClear"/,
		);
		expect(read('QueueList.vue')).toMatch(
			/confirmClear\(\) \{\s*const ids = this\.pendingClear;\s*this\.closeConfirm\(\);\s*this\.\$emit\('clear-all', ids\);/,
		);
	});

	it('lets Esc cancel the confirmation and mutes the global shortcuts meanwhile', () => {
		expect(template('QueueList.vue')).toContain(
			'@keydown.esc.stop.prevent="closeConfirm"',
		);
		expect(read('App.vue')).toMatch(
			/onKeydown\(event: KeyboardEvent\) \{\s*if \(this\.confirmingClear\) \{[\s\S]*?return;/,
		);
	});

	it('counts the visible pending cards and sends exactly those ids once', () => {
		const app = read('App.vue');
		expect(template('App.vue')).toMatch(
			/:clear-ids="clearIds"[\s\S]*?@clear-all="clearAll"/,
		);
		expect(app).toMatch(
			/clearIds\(\): string\[\] \{\s*return this\.order\.filter\(isPending\)\.map/,
		);
		expect((app.match(/decisionsDismissAll\(ids\)/g) ?? []).length).toBe(1);
		expect(read('QueueList.vue')).toContain("this.$emit('clear-all', ids)");
		expect(readFileSync(join(__dirname, '..', 'api.ts'), 'utf8')).toContain(
			"invoke<number>('decisions_dismiss_all', { ids })",
		);
	});
});

describe('Answer guard', () => {
	it('sends the card revision the owner saw with every answer', () => {
		expect(read('App.vue')).toContain(
			'answer.seen_ms = card.updated_ms ?? card.created_ms;',
		);
	});
});
