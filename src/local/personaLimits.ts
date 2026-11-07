import $axios from '@/plugins/axios';

export const DEFAULT_MAX_TOKENS = 4096;
export const DEFAULT_MAX_ANSWER_CHARS = 20000;
export const ASK_TOTAL_TIMEOUT_MS = 15 * 60 * 1000;
const LIMITS_TTL_MS = 5 * 60 * 1000;

export interface PersonaLimits {
	maxTokens: number;
	maxAnswerChars: number;
}

const pick = (value: unknown, fallback: number, min: number, max: number) => {
	const n = Number(value);
	return Number.isInteger(n) && n >= min && n <= max ? n : fallback;
};

let cached: { at: number; limits: PersonaLimits } | null = null;

export const fetchPersonaLimits = async (): Promise<PersonaLimits> => {
	if (cached && Date.now() - cached.at < LIMITS_TTL_MS) return cached.limits;
	try {
		const { data } = await $axios.get('persona-llm/limits');
		const limits = {
			maxTokens: pick(data?.data?.max_tokens, DEFAULT_MAX_TOKENS, 256, 65536),
			maxAnswerChars: pick(
				data?.data?.max_answer_chars,
				DEFAULT_MAX_ANSWER_CHARS,
				1000,
				200000,
			),
		};
		cached = { at: Date.now(), limits };
		return limits;
	} catch {
		return {
			maxTokens: DEFAULT_MAX_TOKENS,
			maxAnswerChars: DEFAULT_MAX_ANSWER_CHARS,
		};
	}
};

export const resetPersonaLimitsCache = () => {
	cached = null;
};
