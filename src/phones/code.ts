export const CODE_LENGTH = 6;

export const sanitizeCode = (value: string): string =>
	value.replace(/\D/g, '').slice(0, CODE_LENGTH);

export const isValidCode = (value: string): boolean =>
	new RegExp(`^\\d{${CODE_LENGTH}}$`).test(value);
