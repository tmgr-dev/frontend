const FORBIDDEN_NAME_CHARS = /[/\\:*?"<>|\u0000-\u001f\u007f]/g;
const MAX_NAME_LENGTH = 100;

export const safeEntryPath = (raw: string): string | null => {
	if (!raw || raw.includes('\u0000')) return null;
	const value = raw.replace(/\\/g, '/');
	if (value.startsWith('/') || /^[A-Za-z]:/.test(value)) return null;
	const segments: string[] = [];
	for (const segment of value.split('/')) {
		if (segment === '' || segment === '.') continue;
		if (segment === '..') return null;
		segments.push(segment);
	}
	return segments.length ? segments.join('/') : null;
};

export const isHiddenPath = (path: string): boolean =>
	path.split('/').some((segment, index) => {
		if (index === 0 && segment === '__MACOSX') return true;
		return segment.startsWith('.');
	});

export const sanitizeFileName = (name: string): string => {
	const cleaned = name
		.replace(FORBIDDEN_NAME_CHARS, '-')
		.trim()
		.replace(/^\.+/, (dots) => '-'.repeat(dots.length))
		.replace(/[. ]+$/, '')
		.slice(0, MAX_NAME_LENGTH)
		.trim();
	return cleaned || 'Untitled';
};

export const uniqueName = (base: string, taken: Set<string>): string => {
	let name = base;
	let index = 2;
	while (taken.has(name.toLowerCase())) {
		name = `${base} (${index})`;
		index += 1;
	}
	taken.add(name.toLowerCase());
	return name;
};

export const encodePath = (path: string): string =>
	path
		.split('/')
		.map((segment) =>
			encodeURIComponent(segment).replace(
				/[()]/g,
				(char) => `%${char.charCodeAt(0).toString(16).toUpperCase()}`,
			),
		)
		.join('/');

export const decodePath = (path: string): string => {
	try {
		return decodeURIComponent(path);
	} catch {
		return path;
	}
};

export const dirName = (path: string): string => {
	const index = path.lastIndexOf('/');
	return index === -1 ? '' : path.slice(0, index);
};

export const baseName = (path: string): string =>
	path.slice(path.lastIndexOf('/') + 1);

export const extName = (name: string): string => {
	const index = name.lastIndexOf('.');
	return index <= 0 ? '' : name.slice(index).toLowerCase();
};

export const stripExt = (name: string): string => {
	const ext = extName(name);
	return ext ? name.slice(0, -ext.length) : name;
};

export const relativePath = (fromFile: string, toFile: string): string => {
	const from = dirName(fromFile).split('/').filter(Boolean);
	const to = toFile.split('/');
	let common = 0;
	while (
		common < from.length &&
		common < to.length - 1 &&
		from[common] === to[common]
	) {
		common += 1;
	}
	const ups = from.slice(common).map(() => '..');
	return [...ups, ...to.slice(common)].join('/');
};

export const resolveRelative = (
	fromFile: string,
	target: string,
): string | null => {
	if (target.includes('\u0000')) return null;
	const stack = target.startsWith('/')
		? []
		: dirName(fromFile).split('/').filter(Boolean);
	for (const segment of target.replace(/\\/g, '/').split('/')) {
		if (segment === '' || segment === '.') continue;
		if (segment === '..') {
			if (!stack.length) return null;
			stack.pop();
		} else {
			stack.push(segment);
		}
	}
	return stack.length ? stack.join('/') : null;
};
