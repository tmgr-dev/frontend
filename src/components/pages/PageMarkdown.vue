<template>
	<div
		ref="rootRef"
		class="page-markdown page-prose"
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
	import './editor/page-editor.css';
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
