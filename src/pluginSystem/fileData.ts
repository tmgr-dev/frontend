export const MAX_FILE_BYTES = 5 * 1024 * 1024;

const TEXT_TYPES =
	/^(text\/|application\/(json|xml|javascript|x-yaml|yaml|csv))/;

const toBase64 = (bytes: Uint8Array) => {
	let binary = '';
	for (let i = 0; i < bytes.length; i += 0x8000) {
		binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	}
	return btoa(binary);
};

/** Plugins have no atob or TextDecoder: they get base64 always, and text when the file is valid UTF-8 text. */
export const encodeFile = (
	bytes: Uint8Array,
	mimeType: string | null,
	name: string,
) => {
	if (bytes.length > MAX_FILE_BYTES)
		throw new Error('the file is larger than 5 MB');
	let text: string | null = null;
	if (
		TEXT_TYPES.test(mimeType ?? '') ||
		/\.(txt|md|csv|json|ya?ml|xml|log)$/i.test(name)
	) {
		try {
			text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
		} catch {
			text = null;
		}
	}
	return { size: bytes.length, base64: toBase64(bytes), text };
};
