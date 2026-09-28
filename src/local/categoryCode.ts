/**
 * Category code derivation, ported from the Java backend's `ProjectCategoryService` /
 * `WorkspaceCodes` (workspaces/src/main/java/dev/tmgr/backend/workspaces/service) so a
 * local category created without an explicit code gets the same CODE-N as cloud would.
 */

const CYRILLIC_TRANSLIT: Record<string, string> = {
	а: 'a', б: 'b', в: 'v', г: 'g', д: 'd',
	е: 'e', ё: 'e', ж: 'zh', з: 'z', и: 'i',
	й: 'y', к: 'k', л: 'l', м: 'm', н: 'n',
	о: 'o', п: 'p', р: 'r', с: 's', т: 't',
	у: 'u', ф: 'f', х: 'h', ц: 'ts', ч: 'ch',
	ш: 'sh', щ: 'sch', ъ: '', ы: 'y', ь: '',
	э: 'e', ю: 'yu', я: 'ya',
	А: 'A', Б: 'B', В: 'V', Г: 'G', Д: 'D',
	Е: 'E', Ё: 'E', Ж: 'Zh', З: 'Z', И: 'I',
	Й: 'Y', К: 'K', Л: 'L', М: 'M', Н: 'N',
	О: 'O', П: 'P', Р: 'R', С: 'S', Т: 'T',
	У: 'U', Ф: 'F', Х: 'H', Ц: 'Ts', Ч: 'Ch',
	Ш: 'Sh', Щ: 'Sch', Ъ: '', Ы: 'Y', Ь: '',
	Э: 'E', Ю: 'Yu', Я: 'Ya',
};

const transliterate = (input: string): string =>
	Array.from(input)
		.map((ch) => CYRILLIC_TRANSLIT[ch] ?? ch)
		.join('');

const MAX_CODE_LENGTH = 50;

/**
 * Sanitizes raw input into a code segment: transliterates Cyrillic to Latin, replaces
 * non-alphanumerics with hyphens, collapses repeats, trims edges and uppercases. Falls
 * back to `CAT` when nothing usable remains.
 */
export const sanitizeCategoryCode = (input: string | null | undefined): string => {
	if (!input) return 'CAT';
	const hyphenated = transliterate(input)
		.replace(/[^a-zA-Z0-9]+/g, '-')
		.replace(/-+/g, '-')
		.replace(/^-+|-+$/g, '');
	return hyphenated ? hyphenated.toUpperCase() : 'CAT';
};

/**
 * Pure sequencing logic for a unique code. Given a sanitized base and the codes already
 * present, returns `BASE` if free, otherwise `BASE-N` with the smallest free N, truncating
 * the base so the total length stays within 50 characters.
 */
export const generateUniqueCategoryCode = (
	sanitizedBase: string | null | undefined,
	existingCodes: Iterable<string>,
): string => {
	let base = sanitizedBase ? sanitizedBase.toUpperCase() : 'CAT';
	if (base.length > MAX_CODE_LENGTH) base = base.slice(0, MAX_CODE_LENGTH);
	const existing = new Set(Array.from(existingCodes, (code) => code.toUpperCase()));
	if (!existing.has(base)) return base;
	let i = 1;
	// eslint-disable-next-line no-constant-condition
	while (true) {
		const suffix = `-${i}`;
		const maxBaseLen = MAX_CODE_LENGTH - suffix.length;
		const truncated = base.length > maxBaseLen ? base.slice(0, maxBaseLen) : base;
		const candidate = truncated + suffix;
		if (!existing.has(candidate)) return candidate;
		i++;
	}
};
