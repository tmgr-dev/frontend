export const AI_WRITE_MODE_KEY = 'ai_write_mode';

export type AiWriteMode = 'read_only' | 'write_with_confirmation';

export const parseAiWriteMode = (value: unknown): AiWriteMode =>
	value === 'write_with_confirmation' ? value : 'read_only';
