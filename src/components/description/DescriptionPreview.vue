<template>
	<div class="relative">
		<Button
			type="button"
			variant="ghost"
			size="icon"
			title="Edit description"
			aria-label="Edit description"
			class="absolute top-1.5 right-1.5 z-10 h-7 w-7 text-ink-muted hover:bg-surface-hover hover:text-ink"
			@click="$emit('edit')"
		>
			<Pencil class="h-4 w-4" />
		</Button>
		<button
			v-if="isEmpty"
			type="button"
			class="w-full px-4 py-3 text-left text-sm text-ink-subtle hover:text-ink-muted"
			@click="$emit('edit')"
		>
			No description — click to add
		</button>
		<div v-else-if="editorType === 'block'" class="px-4 py-3 pr-10">
			<div
				class="description-blocks text-sm leading-relaxed text-ink"
				v-html="blockHtml"
			></div>
		</div>
		<div v-else class="px-4 py-3 pr-10">
			<DescriptionMarkdown :content="description || ''" />
		</div>
	</div>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import type { EditorType } from '@/types';
	import { getBlockEditorDescription } from '@/utils/editor';
	import editorJsToHtml, { legacyHtmlToHtml } from '@/utils/editorJsToHtml';
	import { hasBlockContent, hasMarkdownContent } from '@/utils/editorType';
	import { Pencil } from 'lucide-vue-next';
	import {
		computed,
		defineAsyncComponent,
		defineComponent,
		type PropType,
	} from 'vue';

	export default defineComponent({
		name: 'DescriptionPreview',
		components: {
			Button,
			Pencil,
			DescriptionMarkdown: defineAsyncComponent(
				() => import('./DescriptionMarkdown.vue'),
			),
		},
		props: {
			editorType: { type: String as PropType<EditorType>, required: true },
			description: { type: String as PropType<string | null>, default: null },
			descriptionJson: {
				type: [Object, String] as PropType<Record<string, any> | string | null>,
				default: null,
			},
		},
		emits: ['edit'],
		setup(props) {
			const blockHtml = computed(() => {
				if (props.editorType !== 'block') return '';
				if (hasBlockContent(props.descriptionJson))
					return editorJsToHtml(props.descriptionJson);
				const legacyHtml = legacyHtmlToHtml(props.descriptionJson);
				if (legacyHtml) return legacyHtml;
				return hasMarkdownContent(props.description)
					? editorJsToHtml(getBlockEditorDescription(props.description))
					: '';
			});
			const isEmpty = computed(() =>
				props.editorType === 'block'
					? !blockHtml.value
					: !hasMarkdownContent(props.description),
			);
			return { blockHtml, isEmpty };
		},
	});
</script>

<style scoped>
	.description-blocks {
		overflow-wrap: anywhere;
	}
	.description-blocks :deep(> *:first-child) {
		margin-top: 0;
	}
	.description-blocks :deep(> *:last-child) {
		margin-bottom: 0;
	}
	.description-blocks :deep(p),
	.description-blocks :deep(figure),
	.description-blocks :deep(.warning) {
		margin: 0 0 0.6rem;
	}
	.description-blocks :deep(h1),
	.description-blocks :deep(h2),
	.description-blocks :deep(h3),
	.description-blocks :deep(h4),
	.description-blocks :deep(h5),
	.description-blocks :deep(h6) {
		margin: 1rem 0 0.4rem;
		font-weight: 600;
		line-height: 1.3;
	}
	.description-blocks :deep(h1) {
		font-size: 1.375rem;
	}
	.description-blocks :deep(h2) {
		font-size: 1.125rem;
	}
	.description-blocks :deep(h3) {
		font-size: 1rem;
	}
	.description-blocks :deep(ul),
	.description-blocks :deep(ol) {
		margin: 0 0 0.6rem;
		padding-left: 1.35rem;
	}
	.description-blocks :deep(ul) {
		list-style: disc;
	}
	.description-blocks :deep(ol) {
		list-style: decimal;
	}
	.description-blocks :deep(li) {
		margin: 0.15rem 0;
	}
	.description-blocks :deep(li > ul),
	.description-blocks :deep(li > ol) {
		margin: 0.15rem 0 0;
	}
	.description-blocks :deep(ul.checklist) {
		list-style: none;
		padding-left: 0;
	}
	.description-blocks :deep(ul.checklist li) {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}
	.description-blocks :deep(blockquote) {
		margin: 0 0 0.6rem;
		border-left: 3px solid var(--brand-color);
		padding: 0.1rem 0 0.1rem 0.7rem;
		color: var(--fg-muted);
	}
	.description-blocks :deep(blockquote p) {
		margin: 0;
	}
	.description-blocks :deep(figcaption) {
		margin-top: 0.25rem;
		color: var(--fg-subtle);
		font-size: 0.8125rem;
	}
	.description-blocks :deep(.warning) {
		border: 1px solid var(--line-strong-color);
		border-radius: 0.5rem;
		background: color-mix(in oklab, currentColor 5%, transparent);
		padding: 0.6rem 0.75rem;
	}
	.description-blocks :deep(.warning p) {
		margin: 0.2rem 0 0;
	}
	.description-blocks :deep(code) {
		border-radius: 0.25rem;
		background: color-mix(in oklab, currentColor 10%, transparent);
		padding: 0.1em 0.35em;
		font-size: 0.85em;
	}
	.description-blocks :deep(pre) {
		margin: 0 0 0.6rem;
		overflow-x: auto;
		border: 1px solid var(--line-strong-color);
		border-radius: 0.5rem;
		background: color-mix(in oklab, currentColor 5%, transparent);
		padding: 0.6rem 0.75rem;
		line-height: 1.5;
	}
	.description-blocks :deep(pre code) {
		background: none;
		padding: 0;
		font-size: 0.8125rem;
	}
	.description-blocks :deep(mark) {
		border-radius: 0.125rem;
		background: color-mix(in oklab, var(--brand-color) 25%, transparent);
		color: inherit;
	}
	.description-blocks :deep(hr) {
		margin: 1rem 0;
		border: 0;
		border-top: 1px solid var(--line-strong-color);
	}
	.description-blocks :deep(table) {
		margin: 0 0 0.6rem;
		border-collapse: collapse;
		width: 100%;
	}
	.description-blocks :deep(th),
	.description-blocks :deep(td) {
		border: 1px solid var(--line-strong-color);
		padding: 0.3rem 0.55rem;
		text-align: left;
	}
	.description-blocks :deep(th) {
		background: color-mix(in oklab, currentColor 6%, transparent);
		font-weight: 600;
	}
	.description-blocks :deep(a) {
		color: var(--brand-color);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.description-blocks :deep(img) {
		border-radius: 0.375rem;
		max-width: 100%;
	}
</style>
