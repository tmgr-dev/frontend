import type { LocaleCode } from './locale';

type Tree = Record<string, unknown>;

const AREA = /\/locales\/[^/]+\/([^/]+)\.json$/;

const toTree = (modules: Record<string, unknown>): Tree =>
	Object.fromEntries(
		Object.entries(modules).map(([path, json]) => [
			path.match(AREA)?.[1] ?? path,
			json,
		]),
	);

const english = import.meta.glob<Tree>('../locales/en/*.json', {
	eager: true,
	import: 'default',
});

const others = import.meta.glob<Tree>(
	['../locales/*/*.json', '!../locales/en/*.json'],
	{ import: 'default' },
);

export const englishMessages: Tree = toTree(english);

export const loadMessages = async (locale: LocaleCode): Promise<Tree> => {
	const prefix = `../locales/${locale}/`;
	const entries = await Promise.all(
		Object.entries(others)
			.filter(([path]) => path.startsWith(prefix))
			.map(async ([path, load]) => [path, await load()] as const),
	);
	return toTree(Object.fromEntries(entries));
};
