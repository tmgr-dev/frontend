import { fetchPersonaLimits, resetPersonaLimitsCache } from '../personaLimits';

const mockGet = jest.fn();
jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: (...args: unknown[]) => mockGet(...args) },
}));

beforeEach(() => {
	mockGet.mockReset();
	resetPersonaLimitsCache();
});

it('returns the platform values and caches them', async () => {
	mockGet.mockResolvedValue({
		data: { data: { max_tokens: 8000, max_answer_chars: 50000 } },
	});
	expect(await fetchPersonaLimits()).toEqual({
		maxTokens: 8000,
		maxAnswerChars: 50000,
	});
	await fetchPersonaLimits();
	expect(mockGet).toHaveBeenCalledTimes(1);
	expect(mockGet).toHaveBeenCalledWith('persona-llm/limits');
});

it('falls back to the defaults when the backend is unavailable', async () => {
	mockGet.mockRejectedValue(new Error('offline'));
	expect(await fetchPersonaLimits()).toEqual({
		maxTokens: 4096,
		maxAnswerChars: 20000,
	});
});

it('ignores out-of-range values', async () => {
	mockGet.mockResolvedValue({
		data: { data: { max_tokens: 0, max_answer_chars: 99999999 } },
	});
	expect(await fetchPersonaLimits()).toEqual({
		maxTokens: 4096,
		maxAnswerChars: 20000,
	});
});
