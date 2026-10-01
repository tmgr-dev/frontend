<template>
	<div
		ref="rootRef"
		class="page-markdown break-words text-sm leading-relaxed text-ink"
		@click="onClick"
		v-html="html"
	></div>
</template>

<script lang="ts">
	import { fileDisplayUrl, releaseFileDisplayUrl } from '@/actions/tmgr/files';
	import { renderPageMarkdown, sanitizePageHtml } from '@/utils/pages/render';
	import { extractTmgrRefs, parseTmgrUrl } from '@/utils/pages/tmgrLinks';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		type PropType,
		ref,
		watch,
		watchEffect,
	} from 'vue';
	import type { TmgrDirectory } from './useTmgrDirectory';

	export default defineComponent({
		name: 'PageMarkdown',
		props: {
			markdown: { type: String, default: '' },
			directory: { type: Object as PropType<TmgrDirectory>, required: true },
		},
		emits: ['navigate'],
		setup(props, { emit }) {
			const rootRef = ref<HTMLElement | null>(null);
			const html = computed(() =>
				sanitizePageHtml(renderPageMarkdown(props.markdown)),
			);
			const blobUrls = new Set<string>();

			watch(
				() => props.markdown,
				(markdown) => {
					void props.directory.ensure(extractTmgrRefs(markdown));
				},
				{ immediate: true },
			);

			watchEffect(
				() => {
					const root = rootRef.value;
					if (!root || !html.value) return;
					root
						.querySelectorAll<HTMLElement>('a[data-tmgr]')
						.forEach((anchor) => {
							const parsed = parseTmgrUrl(
								anchor.getAttribute('data-tmgr') || '',
							);
							if (parsed?.form !== 'storage') return;
							const title = props.directory.titleFor(parsed.kind, parsed.id);
							if (title) anchor.textContent = title;
						});
					root
						.querySelectorAll<HTMLImageElement>('img[data-tmgr-file]')
						.forEach((img) => {
							const id = Number(img.getAttribute('data-tmgr-file'));
							if (!id || img.getAttribute('src')) return;
							fileDisplayUrl(id)
								.then((url) => {
									if (url.startsWith('blob:')) blobUrls.add(url);
									img.src = url;
								})
								.catch(() => undefined);
						});
				},
				{ flush: 'post' },
			);

			const onClick = (event: MouseEvent) => {
				const anchor = (event.target as HTMLElement | null)?.closest?.(
					'a[data-tmgr]',
				);
				if (!anchor) return;
				event.preventDefault();
				const parsed = parseTmgrUrl(anchor.getAttribute('data-tmgr') || '');
				if (parsed) emit('navigate', parsed);
			};

			onBeforeUnmount(() => {
				blobUrls.forEach((url) => releaseFileDisplayUrl(url));
				blobUrls.clear();
			});

			return { rootRef, html, onClick };
		},
	});
</script>

<style scoped>
	.page-markdown :deep(> *:first-child) {
		margin-top: 0;
	}
	.page-markdown :deep(> *:last-child) {
		margin-bottom: 0;
	}
	.page-markdown :deep(p),
	.page-markdown :deep(ul),
	.page-markdown :deep(ol) {
		margin: 0 0 0.6rem;
	}
	.page-markdown :deep(ul) {
		list-style: disc;
		padding-left: 1.25rem;
	}
	.page-markdown :deep(ol) {
		list-style: decimal;
		padding-left: 1.25rem;
	}
	.page-markdown :deep(h1),
	.page-markdown :deep(h2),
	.page-markdown :deep(h3) {
		margin: 0.9rem 0 0.4rem;
		font-weight: 600;
	}
	.page-markdown :deep(h2) {
		font-size: 1.15rem;
	}
	.page-markdown :deep(code) {
		border-radius: 0.25rem;
		background: rgb(127 127 127 / 0.15);
		padding: 0.05rem 0.3rem;
		font-size: 0.85em;
	}
	.page-markdown :deep(pre) {
		overflow-x: auto;
		border-radius: 0.375rem;
		background: rgb(127 127 127 / 0.12);
		padding: 0.6rem;
	}
	.page-markdown :deep(a:not(.tmgr-chip)) {
		color: rgb(37 99 235);
		text-decoration: underline;
	}
	.page-markdown :deep(a.tmgr-chip) {
		border-radius: 9999px;
		background: rgb(59 130 246 / 0.12);
		padding: 0 0.5em;
		color: rgb(29 78 216);
		text-decoration: none;
	}
	.page-markdown :deep(img) {
		max-width: 100%;
		border-radius: 0.375rem;
	}
	:global(.dark) .page-markdown :deep(a:not(.tmgr-chip)) {
		color: rgb(147 197 253);
	}
	:global(.dark) .page-markdown :deep(a.tmgr-chip) {
		background: rgb(96 165 250 / 0.18);
		color: rgb(147 197 253);
	}
</style>
