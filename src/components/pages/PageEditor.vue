<template>
	<div
		ref="rootRef"
		class="page-editor relative"
		:data-ready="ready || holdExpired ? 'true' : 'false'"
		@click.capture="onClick"
	>
		<div ref="mountRef"></div>
		<Teleport to="body">
			<ul
				v-if="mention && (mention.items.length || mention.loading)"
				class="mention-menu fixed z-50 max-h-72 w-72 overflow-y-auto rounded-lg border border-line bg-surface p-1 text-sm shadow-tmgr-md"
				:style="{ left: `${mention.x}px`, top: `${mention.y}px` }"
				role="listbox"
				data-testid="mention-menu"
			>
				<li
					v-for="(item, index) in mention.items"
					:key="`${item.kind}/${item.id}`"
					role="option"
					:aria-selected="index === mention.index"
					class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5"
					:class="
						index === mention.index
							? 'bg-surface-active text-ink'
							: 'text-ink-muted hover:bg-surface-hover dark:text-ink-muted'
					"
					@mousedown.prevent="pick(item)"
				>
					<span
						class="w-14 shrink-0 text-2xs uppercase tracking-wide text-ink-subtle"
						>{{ item.kind }}</span
					>
					<span class="truncate">{{ item.title }}</span>
				</li>
				<li
					v-if="mention.loading && !mention.items.length"
					class="px-2 py-1.5 text-ink-subtle"
				>
					Searching...
				</li>
			</ul>
		</Teleport>
	</div>
</template>

<script lang="ts">
	import { emitPageLinkAdded } from '@/tours/pageRoutineEvents';
	import { extractTmgrRefs, parseTmgrUrl } from '@/utils/pages/tmgrLinks';
	import type { Crepe } from '@milkdown/crepe';
	import '@milkdown/crepe/theme/common/style.css';
	import {
		defineComponent,
		nextTick,
		onBeforeUnmount,
		onMounted,
		type PropType,
		ref,
		watch,
	} from 'vue';
	import { createPageCrepe } from './editor/crepe';
	import { createImageView } from './editor/imageView';
	import { useMentions } from './editor/mentions';
	import './editor/page-editor.css';
	import type { TmgrDirectory } from './useTmgrDirectory';

	export default defineComponent({
		name: 'PageEditor',
		props: {
			modelValue: { type: String, default: '' },
			directory: { type: Object as PropType<TmgrDirectory>, required: true },
			placeholder: { type: String, default: '' },
			uploadFile: {
				type: Function as PropType<(file: File) => Promise<number>>,
				default: undefined,
			},
		},
		emits: ['change', 'navigate', 'upload-error', 'make-task'],
		setup(props, { emit }) {
			const rootRef = ref<HTMLElement | null>(null);
			const mountRef = ref<HTMLElement | null>(null);
			const ready = ref(false);
			const holdExpired = ref(false);
			const holdTimer = setTimeout(() => {
				holdExpired.value = true;
			}, 1500);
			let crepe: Crepe | null = null;
			let baseline = '';
			const { mention, pick, plugin, dispose } = useMentions(
				() => props.directory,
				() => ready.value,
				(item) => emitPageLinkAdded(item.kind),
			);

			watch(
				() => ready.value || holdExpired.value,
				(shown) => {
					if (!shown) return;
					void nextTick(() => {
						const doc = rootRef.value?.closest('.page-doc');
						if (
							doc &&
							!doc.querySelector(':scope > .page-editor[data-ready="false"]')
						) {
							doc.setAttribute('data-shown', '');
						}
					});
				},
			);

			const onClick = (event: MouseEvent) => {
				const target = event.target as HTMLElement | null;
				const anchor = target?.closest?.('a') as HTMLAnchorElement | null;
				if (!anchor) return;
				const href = anchor.getAttribute('data-tmgr');
				if (href === null) {
					if (anchor.getAttribute('href') === '') event.preventDefault();
					return;
				}
				event.preventDefault();
				event.stopPropagation();
				const parsed = parseTmgrUrl(href);
				if (parsed) emit('navigate', parsed);
			};

			onMounted(async () => {
				try {
					await props.directory.ensure(extractTmgrRefs(props.modelValue));
				} catch {
					// titles are cosmetic; the editor still opens without them
				}
				if (!mountRef.value) return;
				const instance = createPageCrepe({
					root: mountRef.value,
					markdown: props.modelValue,
					placeholder: props.placeholder,
					directory: props.directory,
					uploadFile: props.uploadFile,
					onUploadError: (file) => emit('upload-error', file),
					onMakeTask: (text) => emit('make-task', text),
				});
				crepe = instance;
				instance.editor.use(plugin).use(createImageView());
				instance.on((listener) => {
					listener.markdownUpdated((_ctx, markdown) => {
						if (!ready.value) return;
						emit('change', markdown, markdown !== baseline);
					});
				});
				await instance.create();
				if (crepe !== instance) return;
				baseline = instance.getMarkdown();
				ready.value = true;
			});

			onBeforeUnmount(() => {
				clearTimeout(holdTimer);
				dispose();
				ready.value = false;
				crepe?.destroy();
				crepe = null;
			});

			return { rootRef, mountRef, mention, pick, onClick, ready, holdExpired };
		},
	});
</script>
