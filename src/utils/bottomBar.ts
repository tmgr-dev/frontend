/**
 * The task page has two bottom bars side by side: the form's footer and the comment composer in
 * the right rail (TM-229). The footer's height depends on its content, so the composer matches it
 * through a CSS variable rather than a padding that only lines up in one layout.
 */
export function footerHeightVars(height: number): Record<string, string> {
	return Number.isFinite(height) && height > 0 ? { '--task-footer-height': `${height}px` } : {};
}
