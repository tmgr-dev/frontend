export const CLAUDE_MODEL_LABELS: Record<string, string> = {
	opus: 'Opus 5.5',
	sonnet: 'Sonnet 5.5',
	haiku: 'Haiku 5.5',
	fable: 'Fable 5.2',
};

export const modelLabel = (agent: string, value: string): string =>
	agent === 'claude' &&
	Object.prototype.hasOwnProperty.call(CLAUDE_MODEL_LABELS, value)
		? CLAUDE_MODEL_LABELS[value]
		: value;
