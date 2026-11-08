<template>
	<DialogRoot :open="open" @update:open="$emit('update:open', $event)">
		<DialogPortal>
			<DialogOverlay
				class="fixed inset-0 z-50 bg-black/80 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
			/>
			<DialogContent
				class="fixed top-[10vh] left-1/2 z-50 grid max-h-[80vh] w-full max-w-xl -translate-x-1/2 grid-rows-[auto_1fr] gap-3 border bg-background p-4 shadow-lg duration-200 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0 sm:rounded-lg"
				@open-auto-focus.prevent="focusInput"
				@close-auto-focus="onCloseAutoFocus"
				@interact-outside="onInteractOutside"
			>
				<DialogTitle class="sr-only">{{ ui.search }}</DialogTitle>
				<DialogDescription class="sr-only">{{
					ui.searchPlaceholder
				}}</DialogDescription>
				<input
					ref="input"
					v-model="query"
					type="search"
					role="combobox"
					aria-autocomplete="list"
					:aria-expanded="results.length > 0"
					aria-controls="docs-search-results"
					:aria-activedescendant="
						results.length ? `docs-search-result-${active}` : undefined
					"
					:aria-label="ui.search"
					:placeholder="ui.searchPlaceholder"
					class="mr-8 h-10 w-auto rounded-md border border-input bg-background px-3 text-sm text-ink placeholder:text-ink-subtle focus-visible:outline-2 focus-visible:outline-primary"
					data-testid="docs-search-input"
					@keydown="onKeydown"
				/>
				<div class="min-h-0 overflow-y-auto">
					<p
						v-if="query.trim() && !results.length"
						class="px-2 py-6 text-center text-sm text-ink-subtle"
						role="status"
						data-testid="docs-search-empty"
					>
						{{ ui.noResults }}
					</p>
					<ul
						v-else
						id="docs-search-results"
						role="listbox"
						:aria-label="ui.search"
						class="space-y-1"
					>
						<li
							v-for="(result, index) in results"
							:id="`docs-search-result-${index}`"
							:key="`${result.slug}#${result.anchor}`"
							role="option"
							:aria-selected="index === active"
							class="cursor-pointer rounded-md px-3 py-2 text-sm"
							:class="index === active ? 'bg-muted' : 'hover:bg-muted'"
							data-testid="docs-search-result"
							@mousemove="active = index"
							@click="choose(result)"
						>
							<p class="font-medium text-ink">
								{{ result.heading || result.pageTitle }}
							</p>
							<p v-if="result.heading" class="text-xs text-ink-subtle">
								{{ result.pageTitle }}
							</p>
							<p
								v-if="result.snippet"
								class="mt-0.5 line-clamp-2 text-xs text-ink-subtle"
							>
								{{ result.snippet }}
							</p>
						</li>
					</ul>
				</div>
				<DialogClose
					class="absolute top-4 right-4 rounded-sm opacity-70 transition-opacity hover:opacity-100"
					data-testid="docs-search-close"
				>
					<X class="h-4 w-4" aria-hidden="true" />
					<span class="sr-only">{{ ui.close }}</span>
				</DialogClose>
			</DialogContent>
		</DialogPortal>
	</DialogRoot>
</template>

<script lang="ts">
	import { isTourLayerEvent } from '@/tours/a11y';
	import { X } from 'lucide-vue-next';
	import {
		DialogClose,
		DialogContent,
		DialogDescription,
		DialogOverlay,
		DialogPortal,
		DialogRoot,
		DialogTitle,
	} from 'radix-vue';
	import {
		searchDocs,
		type DocsSearchEntry,
		type DocsSearchResult,
	} from '@/utils/docs';
	import { computed, defineComponent, ref, watch, type PropType } from 'vue';

	export default defineComponent({
		name: 'DocsSearch',
		components: {
			DialogClose,
			DialogContent,
			DialogDescription,
			DialogOverlay,
			DialogPortal,
			DialogRoot,
			DialogTitle,
			X,
		},
		props: {
			open: { type: Boolean, required: true },
			entries: { type: Array as PropType<DocsSearchEntry[]>, required: true },
			ui: {
				type: Object as PropType<{
					search: string;
					close: string;
					searchPlaceholder: string;
					noResults: string;
				}>,
				required: true,
			},
		},
		emits: ['update:open', 'select'],
		setup(props, { emit }) {
			const input = ref<HTMLInputElement | null>(null);
			const query = ref('');
			const active = ref(0);
			let chosen = false;
			const results = computed(() => searchDocs(props.entries, query.value));

			watch(results, () => (active.value = 0));
			watch(
				() => props.open,
				(open) => {
					if (open) chosen = false;
				},
			);

			const scrollActive = () =>
				document
					.getElementById(`docs-search-result-${active.value}`)
					?.scrollIntoView({ block: 'nearest' });

			const choose = (result: DocsSearchResult) => {
				chosen = true;
				emit('update:open', false);
				emit('select', result);
			};

			const onKeydown = (event: KeyboardEvent) => {
				const count = results.value.length;
				if (event.key === 'ArrowDown' && count) {
					event.preventDefault();
					active.value = (active.value + 1) % count;
					scrollActive();
				} else if (event.key === 'ArrowUp' && count) {
					event.preventDefault();
					active.value = (active.value - 1 + count) % count;
					scrollActive();
				} else if (event.key === 'Enter' && count) {
					event.preventDefault();
					choose(results.value[active.value]);
				}
			};

			const focusInput = () => {
				input.value?.focus();
				input.value?.select();
			};

			const onCloseAutoFocus = (event: Event) => {
				if (chosen) event.preventDefault();
			};

			const onInteractOutside = (event: Event) => {
				if (isTourLayerEvent(event)) event.preventDefault();
			};

			return {
				input,
				query,
				active,
				results,
				choose,
				onKeydown,
				focusInput,
				onCloseAutoFocus,
				onInteractOutside,
			};
		},
	});
</script>
