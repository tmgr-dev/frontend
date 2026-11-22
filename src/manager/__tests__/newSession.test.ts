import {
	applyTemplate,
	distinctChats,
	machineOptions,
	SESSION_TEMPLATES,
} from '../newSession';
import type { ChatMessage, SpawnRequest } from '../types';

const form: SpawnRequest = {
	agent: 'claude',
	machine: 'mac',
	folder: '',
	name: '',
	task: '',
	prompt: 'old',
};

describe('machineOptions', () => {
	it('labels the local machine This Mac and marks others linked', () => {
		const out = machineOptions(
			['mac', 'abcdefghijklmnop', 'studio'],
			[
				{ machine: 'mac', local: true },
				{ machine: 'studio', label: 'Studio' },
			],
		);
		expect(out).toEqual([
			{ id: 'mac', label: 'This Mac', linked: false },
			{ id: 'abcdefghijklmnop', label: 'abcdefghijkl', linked: false },
			{ id: 'studio', label: 'Studio', linked: true },
		]);
	});

	it('falls back to ids without pairing data', () => {
		expect(machineOptions(['mac'], [])).toEqual([
			{ id: 'mac', label: 'mac', linked: false },
		]);
	});
});

describe('applyTemplate', () => {
	it('blank clears the prompt, others fill it, unknown keeps the form', () => {
		expect(applyTemplate(form, 'blank').prompt).toBe('');
		for (const t of SESSION_TEMPLATES.slice(1))
			expect(applyTemplate(form, t.id).prompt).toBe(t.prompt);
		expect(applyTemplate(form, 'nope')).toBe(form);
	});
});

describe('distinctChats', () => {
	it('counts distinct rooms among hits', () => {
		const hit = (session?: string) =>
			({
				room: session
					? { kind: 'session', machine: 'mac', session }
					: { kind: 'manager', machine: 'mac' },
			}) as ChatMessage;
		expect(distinctChats([hit(), hit(), hit('a'), hit('a'), hit('b')])).toBe(3);
		expect(distinctChats([])).toBe(0);
	});
});
