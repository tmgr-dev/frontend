const randomHex = (bytes: number) =>
	Array.from({ length: bytes }, () =>
		Math.floor(Math.random() * 256)
			.toString(16)
			.padStart(2, '0'),
	).join('');

const generate = (): string => {
	if (typeof crypto !== 'undefined' && crypto.randomUUID)
		return crypto.randomUUID();
	const variant = (8 + Math.floor(Math.random() * 4)).toString(16);
	const hex = randomHex(16);
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-${variant}${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
};

/** Per JS context and never persisted: two tabs of one user must differ, or a plugin leader tab would drop the other tab's edits. */
export const SOURCE_INSTANCE_ID = generate();

export const SOURCE_INSTANCE_HEADER = 'X-Source-Instance-Id';
