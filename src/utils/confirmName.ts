export const isConfirmNameMatch = (typed: string, expected: string): boolean =>
	expected.trim() !== '' && typed.trim() === expected.trim();
