<template>
	<div
		class="markdown-text break-words text-sm leading-relaxed text-ink"
		v-html="html"
		@click="onClick"
	></div>
</template>

<script>
	import { defineComponent } from 'vue';
	import { markdownToHtml } from '@/utils/markdown';
	import sanitizeHtml from '@/utils/sanitizeHtml';
	import { openTaskByKey, useTaskKeyPrefixes } from '@/composable/useTaskKeyLinks';

	export default defineComponent({
		name: 'MarkdownText',
		props: {
			content: {
				type: String,
				default: '',
			},
			linkTaskKeys: {
				type: Boolean,
				default: true,
			},
		},
		setup() {
			return { taskKeyPrefixes: useTaskKeyPrefixes() };
		},
		computed: {
			html() {
				// Sanitized after rendering, never before: marked passes embedded
				// HTML through, and comments are written by people.
				return sanitizeHtml(
					markdownToHtml(this.content || '', {
						taskKeyPrefixes: this.linkTaskKeys ? this.taskKeyPrefixes : [],
					}),
				);
			},
		},
		methods: {
			// The markdown is injected as HTML, so the keys inside it are reached
			// by delegation rather than by a listener per link.
			onClick(event) {
				const target = event.target?.closest?.('[data-task-key]');
				if (!target) {
					return;
				}
				event.preventDefault();
				openTaskByKey(target.getAttribute('data-task-key'));
			},
		},
	});
</script>

<style scoped>
	/* Comment-sized typography: readable blocks in a 380px rail, on both themes.
	   Borders use the strong line token — the plain one is 5% alpha and vanishes —
	   and fills are mixed from the current text colour so they follow the theme. */
	.markdown-text {
		overflow-wrap: anywhere;
	}
	.markdown-text :deep(> *:first-child) {
		margin-top: 0;
	}
	.markdown-text :deep(> *:last-child) {
		margin-bottom: 0;
	}
	.markdown-text :deep(p) {
		margin: 0 0 0.55rem;
	}
	.markdown-text :deep(h1),
	.markdown-text :deep(h2),
	.markdown-text :deep(h3),
	.markdown-text :deep(h4),
	.markdown-text :deep(h5),
	.markdown-text :deep(h6) {
		margin: 0.9rem 0 0.35rem;
		font-weight: 600;
		line-height: 1.3;
	}
	.markdown-text :deep(h1) {
		font-size: 1.125rem;
	}
	.markdown-text :deep(h2) {
		font-size: 1rem;
	}
	.markdown-text :deep(h3),
	.markdown-text :deep(h4),
	.markdown-text :deep(h5),
	.markdown-text :deep(h6) {
		font-size: 0.9375rem;
	}
	.markdown-text :deep(ul),
	.markdown-text :deep(ol) {
		margin: 0 0 0.55rem;
		padding-left: 1.35rem;
	}
	.markdown-text :deep(ul) {
		list-style: disc;
	}
	.markdown-text :deep(ol) {
		list-style: decimal;
	}
	.markdown-text :deep(ul ul) {
		list-style: circle;
	}
	.markdown-text :deep(li) {
		margin: 0.15rem 0;
	}
	.markdown-text :deep(li > ul),
	.markdown-text :deep(li > ol) {
		margin: 0.15rem 0 0;
	}
	.markdown-text :deep(blockquote) {
		margin: 0 0 0.55rem;
		border-left: 3px solid var(--brand-color);
		padding: 0.1rem 0 0.1rem 0.7rem;
		color: var(--fg-muted);
	}
	.markdown-text :deep(code) {
		border-radius: 0.25rem;
		background: color-mix(in oklab, currentColor 10%, transparent);
		padding: 0.1em 0.35em;
		font-size: 0.85em;
	}
	.markdown-text :deep(pre) {
		margin: 0 0 0.55rem;
		overflow-x: auto;
		border: 1px solid var(--line-strong-color);
		border-radius: 0.5rem;
		background: color-mix(in oklab, currentColor 5%, transparent);
		padding: 0.6rem 0.75rem;
		line-height: 1.5;
	}
	.markdown-text :deep(pre code) {
		background: none;
		padding: 0;
		font-size: 0.8125rem;
	}
	.markdown-text :deep(hr) {
		margin: 0.9rem 0;
		border: 0;
		border-top: 1px solid var(--line-strong-color);
	}
	.markdown-text :deep(.table-scroll) {
		margin: 0 0 0.55rem;
		overflow-x: auto;
	}
	.markdown-text :deep(table) {
		border-collapse: collapse;
		width: 100%;
	}
	.markdown-text :deep(th),
	.markdown-text :deep(td) {
		border: 1px solid var(--line-strong-color);
		padding: 0.3rem 0.55rem;
		text-align: left;
	}
	.markdown-text :deep(th) {
		background: color-mix(in oklab, currentColor 6%, transparent);
		font-weight: 600;
	}
	.markdown-text :deep(a) {
		color: var(--brand-color);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.markdown-text :deep(img) {
		margin-top: 0.35rem;
		border-radius: 0.375rem;
		max-width: 100%;
	}
	.markdown-text :deep(.task-key) {
		border-radius: 0.25rem;
		background: var(--brand-bg-color);
		padding: 0.05rem 0.35rem;
		color: var(--brand-fg-color);
		font-weight: 500;
		font-size: 0.875em;
	}
	.markdown-text :deep(.task-key:hover) {
		text-decoration: underline;
	}
</style>
