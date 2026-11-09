import { readFileSync } from 'fs';
import { join } from 'path';

const read = (name: string): string =>
	readFileSync(join(__dirname, '..', name), 'utf8');

describe('draft wiring', () => {
	const app = read('components/App.vue');
	const chat = read('components/ChatPane.vue');
	const card = read('components/CardPane.vue');

	it('chat input is driven by the persisted draft, not local state', () => {
		expect(chat).not.toMatch(/data\(\) \{\s*return \{ text: ''/);
		expect(chat).toContain("'update:draft'");
		expect(app).toContain(':draft="chatDraft"');
	});

	it('chat draft is cleared only after a successful send', () => {
		expect(app).toMatch(
			/async sendChat\(text: string\) \{[\s\S]*?if \(await this\.sendMessage\([\s\S]*?\)\) \{[\s\S]*?clearChatDraft/,
		);
		expect(chat).not.toMatch(
			/this\.\$emit\('send'[\s\S]{0,40}\n\s*this\.text = ''/,
		);
	});

	it('free answer and note persist per card', () => {
		expect(card).toContain("'update:draft'");
		expect(app).toContain(':free-draft=');
		expect(app).toContain(':note-draft=');
		expect(app).toContain(':note-for-draft=');
	});

	it('ignores a second chat send while one is in flight', () => {
		expect(app).toContain('sendingChat.includes(card.id)');
	});

	it('prunes drafts after a successful refresh', () => {
		expect(app).toMatch(/pruneDrafts\(/);
	});
});
