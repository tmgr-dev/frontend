import { t } from '@/i18n';
import { addCopyButtons, copyBlockText, copyText } from '@/manager/format';
import { renderMarkdown } from './markdown';

export { copyBlockText, copyText };

const PLAIN_LANGUAGES = new Set(['plaintext', 'text', 'txt']);
const FEEDBACK_MS = 1500;

const ICON =
	'<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 5.5v-2a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 3.5V9A1.5 1.5 0 0 0 4 10.5h1.5"/></svg>';

export const codeLanguage = (className: string): string | null =>
	/(?:^|\s)language-(\S+)/.exec(className)?.[1]?.toLowerCase() ?? null;

export const wrapsLines = (className: string): boolean => {
	const language = codeLanguage(className);
	return language === null || PLAIN_LANGUAGES.has(language);
};

const iconize = (button: Element): void => {
	const label = t('decisions.copy.label');
	button.classList.add('md-copy');
	button.setAttribute('data-md-copy', '');
	button.setAttribute('aria-label', label);
	button.setAttribute('title', label);
	button.setAttribute('data-done', t('decisions.copy.done'));
	button.setAttribute('data-failed', t('decisions.copy.failed'));
	button.innerHTML = ICON;
};

export const decorateCopyable = (html: string): string => {
	if (!html || typeof document === 'undefined') return html;
	const template = document.createElement('template');
	template.innerHTML = html;
	template.content.querySelectorAll('pre').forEach((pre) => {
		const className = pre.querySelector('code')?.getAttribute('class') ?? '';
		if (wrapsLines(className)) pre.classList.add('md-wrap');
	});
	const labels = {
		copy: t('decisions.copy.label'),
		copied: t('decisions.copy.done'),
	};
	const wrapped = document.createElement('template');
	wrapped.innerHTML = addCopyButtons(template.innerHTML, labels);
	wrapped.content.querySelectorAll('.blk').forEach((blk) => {
		blk.classList.add('md-copyable');
	});
	wrapped.content.querySelectorAll('.blk-copy').forEach(iconize);
	return wrapped.innerHTML;
};

export const flashCopied = (button: Element, ok: boolean): void => {
	const state = ok ? 'done' : 'failed';
	button.classList.add(state);
	window.setTimeout(() => button.classList.remove(state), FEEDBACK_MS);
};

export const copyClick = (event: MouseEvent): boolean => {
	const target = event.target as Element | null;
	const button = target?.closest?.('[data-md-copy]');
	if (!button) return false;
	event.preventDefault();
	void copyText(copyBlockText(button.parentElement)).then((ok) =>
		flashCopied(button, ok),
	);
	return true;
};

export const renderWithCopy = (text: string): string =>
	decorateCopyable(renderMarkdown(text));
