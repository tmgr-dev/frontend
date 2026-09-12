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
	.markdown-text :deep(p) {
		margin: 0;
	}
	.markdown-text :deep(p + p),
	.markdown-text :deep(ul),
	.markdown-text :deep(ol),
	.markdown-text :deep(pre),
	.markdown-text :deep(blockquote),
	.markdown-text :deep(table) {
		margin-top: 0.5rem;
	}
	.markdown-text :deep(ul),
	.markdown-text :deep(ol) {
		padding-left: 1.25rem;
		list-style: revert;
	}
	.markdown-text :deep(li + li) {
		margin-top: 0.125rem;
	}
	.markdown-text :deep(.task-key) {
		border-radius: 0.25rem;
		background: rgb(37 99 235 / 10%);
		padding: 0.05rem 0.3rem;
		color: var(--color-brand, #2563eb);
		font-weight: 500;
		font-size: 0.8125rem;
	}
	.markdown-text :deep(.task-key:hover) {
		background: rgb(37 99 235 / 18%);
		text-decoration: underline;
	}
	.markdown-text :deep(a) {
		color: var(--color-brand, #2563eb);
		text-decoration: underline;
	}
	.markdown-text :deep(code) {
		border-radius: 0.25rem;
		background: rgb(0 0 0 / 6%);
		padding: 0.05rem 0.3rem;
		font-size: 0.8125rem;
	}
	.markdown-text :deep(pre) {
		overflow-x: auto;
		border-radius: 0.375rem;
		background: rgb(0 0 0 / 6%);
		padding: 0.5rem 0.75rem;
	}
	.markdown-text :deep(pre code) {
		background: transparent;
		padding: 0;
	}
	.markdown-text :deep(blockquote) {
		border-left: 3px solid rgb(0 0 0 / 15%);
		padding-left: 0.6rem;
		color: inherit;
		opacity: 0.85;
	}
	.markdown-text :deep(h1),
	.markdown-text :deep(h2),
	.markdown-text :deep(h3),
	.markdown-text :deep(h4) {
		margin-top: 0.5rem;
		font-weight: 600;
		font-size: 0.9375rem;
	}
	.markdown-text :deep(table) {
		display: block;
		overflow-x: auto;
		border-collapse: collapse;
	}
	.markdown-text :deep(th),
	.markdown-text :deep(td) {
		border: 1px solid rgb(0 0 0 / 12%);
		padding: 0.2rem 0.45rem;
	}
	.markdown-text :deep(img) {
		max-width: 100%;
	}
	:global(.dark) .markdown-text :deep(code),
	:global(.dark) .markdown-text :deep(pre) {
		background: rgb(255 255 255 / 10%);
	}
	:global(.dark) .markdown-text :deep(th),
	:global(.dark) .markdown-text :deep(td) {
		border-color: rgb(255 255 255 / 18%);
	}
	:global(.dark) .markdown-text :deep(blockquote) {
		border-left-color: rgb(255 255 255 / 25%);
	}
</style>
