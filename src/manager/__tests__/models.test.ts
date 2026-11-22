import { CLAUDE_MODEL_LABELS, modelLabel } from '../models';
import { AGENT_MODELS } from '../store';

describe('claude model labels', () => {
	it('labels every claude alias offered by the switch dialog', () => {
		for (const alias of AGENT_MODELS.claude)
			expect(CLAUDE_MODEL_LABELS[alias]).toEqual(expect.any(String));
	});

	it('offers the claude aliases in label-map order, opus first', () => {
		expect(AGENT_MODELS.claude).toEqual(Object.keys(CLAUDE_MODEL_LABELS));
		expect(AGENT_MODELS.claude[0]).toBe('opus');
	});

	it('maps each claude alias to its display label', () => {
		expect(CLAUDE_MODEL_LABELS).toEqual({
			opus: 'Opus 5.5',
			sonnet: 'Sonnet 5.5',
			haiku: 'Haiku 5.5',
			fable: 'Fable 5.2',
		});
	});

	it('returns the label for claude and the raw value for other agents', () => {
		expect(modelLabel('claude', 'fable')).toBe('Fable 5.2');
		expect(modelLabel('claude', 'haiku')).toBe('Haiku 5.5');
		expect(modelLabel('codex', 'gpt-5')).toBe('gpt-5');
		expect(modelLabel('opencode', 'default')).toBe('default');
		expect(modelLabel('agy', 'default')).toBe('default');
	});

	it('returns an unknown claude value unchanged', () => {
		expect(modelLabel('claude', 'custom-model')).toBe('custom-model');
		expect(modelLabel('claude', 'toString')).toBe('toString');
	});

	it('keeps the current codex model first in the fallback list', () => {
		expect(AGENT_MODELS.codex[0]).toBe('gpt-6.1-terra');
		expect(AGENT_MODELS.codex).not.toContain('gpt-5');
	});
});
