import { loadDocs } from '@/utils/docs';

const files = import.meta.glob('./{en,ru}/**/*.md', {
	eager: true,
	query: '?raw',
	import: 'default',
}) as Record<string, string>;

export const DOCS_PAGES = loadDocs(files);
