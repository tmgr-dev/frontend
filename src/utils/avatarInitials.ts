/** The letters shown while a user has no picture, or none can be loaded (TM-142). */
export function avatarInitials(name: string | null | undefined): string {
	const words = (name ?? '').trim().split(/\s+/).filter(Boolean);

	if (!words.length) {
		return '?';
	}

	return words
		.slice(0, 2)
		.map((word) => word[0].toUpperCase())
		.join('');
}
