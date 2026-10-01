const MAX_LENGTH = 80;
const SUFFIX_LENGTH = 4;
const ALPHABET = 'abcdefghijklmnopqrstuvwxyz0123456789';
const RESERVED = new Set(['tree', 'trash', 'search', 'settings']);

const TRANSLIT: Record<string, string> = {
	а: 'a', б: 'b', в: 'v', г: 'g', д: 'd', е: 'e', ё: 'e', ж: 'zh', з: 'z', и: 'i', й: 'y', к: 'k', л: 'l',
	м: 'm', н: 'n', о: 'o', п: 'p', р: 'r', с: 's', т: 't', у: 'u', ф: 'f', х: 'h', ц: 'ts', ч: 'ch', ш: 'sh',
	щ: 'sch', ъ: '', ы: 'y', ь: '', э: 'e', ю: 'yu', я: 'ya',
};

export const slugify = (title: string): string => {
	let latin = '';
	for (const ch of title) latin += TRANSLIT[ch.toLowerCase()] ?? ch;
	let slug = latin
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
	if (!slug) return 'page';
	if (/^\d+$/.test(slug) || RESERVED.has(slug)) slug = `p-${slug}`;
	const room = MAX_LENGTH - SUFFIX_LENGTH - 1;
	return slug.length > room ? slug.substring(0, room).replace(/-+$/, '') : slug;
};

export const withSuffix = (slug: string): string => {
	const bytes = new Uint8Array(SUFFIX_LENGTH);
	crypto.getRandomValues(bytes);
	let suffix = '';
	for (const byte of bytes) suffix += ALPHABET[byte % ALPHABET.length];
	return `${slug}-${suffix}`;
};
