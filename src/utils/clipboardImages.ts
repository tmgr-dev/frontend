const EXTENSIONS: Record<string, string> = {
	'image/png': 'png',
	'image/jpeg': 'jpg',
	'image/gif': 'gif',
	'image/webp': 'webp',
	'image/avif': 'avif',
	'image/svg+xml': 'svg',
};

/** Images in a paste; a text or non-image paste yields nothing, so the caller can ignore it. */
export function imagesFromClipboard(
	items: DataTransferItemList | DataTransferItem[] | null | undefined,
): File[] {
	if (!items) {
		return [];
	}

	const files: File[] = [];

	for (const item of Array.from(items as ArrayLike<DataTransferItem>)) {
		if (item.kind !== 'file' || !item.type.startsWith('image/')) {
			continue;
		}

		const file = item.getAsFile();

		if (file) {
			files.push(file);
		}
	}

	return files;
}

/**
 * A pasted screenshot arrives as `image.png` whatever it is a picture of, so every attachment
 * would carry the same name. Naming it after the moment it was pasted keeps a list readable.
 */
export function clipboardImageName(mimeType: string, at = new Date()): string {
	const pad = (n: number) => String(n).padStart(2, '0');
	const stamp =
		`${at.getUTCFullYear()}-${pad(at.getUTCMonth() + 1)}-${pad(
			at.getUTCDate(),
		)}` +
		`-${pad(at.getUTCHours())}${pad(at.getUTCMinutes())}${pad(
			at.getUTCSeconds(),
		)}`;

	return `screenshot-${stamp}.${EXTENSIONS[mimeType] ?? 'png'}`;
}
