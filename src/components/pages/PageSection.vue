<template>
	<section
		class="my-3 -ml-3 rounded-lg border-l-4 border-violet-500 bg-violet-50 p-3 dark:bg-violet-900/20"
		:data-section-id="section.id"
		data-testid="managed-section"
		:data-read-only="readOnly ? 'true' : undefined"
	>
		<header class="mb-2 flex items-center justify-between gap-2">
			<span class="inline-flex min-w-0 items-center gap-1.5">
				<AuthorBadge
					v-if="author"
					:author="author"
					:size="20"
					class="text-violet-700 dark:text-violet-300"
				/>
				<span
					v-else
					class="inline-flex items-center gap-1.5 text-sm font-semibold text-violet-700 dark:text-violet-300"
				>
					<component :is="ownerIcon" class="h-4 w-4" />
					{{ label }}
				</span>
				<span
					class="rounded bg-violet-100 px-1.5 py-0.5 text-2xs uppercase tracking-wide text-violet-700 dark:bg-violet-900/40 dark:text-violet-300"
					>{{ section.owner }}</span
				>
			</span>
			<button
				v-if="!editing && !readOnly"
				type="button"
				class="inline-flex items-center gap-1 rounded px-2 py-1 text-xs text-violet-700 hover:bg-violet-100 dark:text-violet-300 dark:hover:bg-violet-900/40"
				@click="startEdit"
			>
				<Pencil class="h-3.5 w-3.5" />
				Edit
			</button>
		</header>

		<div v-if="editing">
			<textarea
				v-model="draft"
				rows="8"
				class="w-full rounded-md border border-gray-300 bg-white p-2 font-mono text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-violet-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
				data-testid="section-textarea"
			></textarea>
			<div class="mt-2 flex justify-end gap-2">
				<button
					type="button"
					class="rounded-md px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700"
					:disabled="saving"
					@click="editing = false"
				>
					Cancel
				</button>
				<button
					type="button"
					class="rounded-md bg-violet-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-violet-700 disabled:opacity-50"
					:disabled="saving"
					@click="save"
				>
					Save
				</button>
			</div>
		</div>
		<PageMarkdown
			v-else
			:markdown="section.inner"
			:directory="directory"
			@navigate="$emit('navigate', $event)"
		/>
	</section>
</template>

<script lang="ts">
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import { ownerAuthorRef, ownerLabel } from '@/utils/pages/author';
	import { parseOwner, type SectionSegment } from '@/utils/pages/sections';
	import { Bot, Cog, Pencil, UserRound } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType, ref, watch } from 'vue';
	import PageMarkdown from './PageMarkdown.vue';
	import type { TmgrDirectory } from './useTmgrDirectory';

	export default defineComponent({
		name: 'PageSection',
		components: { AuthorBadge, PageMarkdown, Pencil },
		props: {
			section: { type: Object as PropType<SectionSegment>, required: true },
			directory: { type: Object as PropType<TmgrDirectory>, required: true },
			saving: { type: Boolean, default: false },
		},
		emits: ['save', 'navigate'],
		setup(props, { emit }) {
			const editing = ref(false);
			const draft = ref('');
			const author = computed(() =>
				ownerAuthorRef(props.section.owner, (kind, id) =>
					props.directory.titleFor(kind, id),
				),
			);
			const label = computed(() => ownerLabel(props.section.owner));
			const readOnly = computed(
				() => parseOwner(props.section.owner).kind === 'system',
			);
			const ownerIcon = computed(() => {
				const { kind } = parseOwner(props.section.owner);
				if (kind === 'system') return Cog;
				if (kind === 'agents') return Bot;
				return UserRound;
			});

			watch(
				() => props.section.owner,
				(owner) => {
					const { kind, ref: id } = parseOwner(owner);
					if (kind === 'persona' && id) {
						void props.directory.ensure([{ kind: 'persona', id }]);
					}
				},
				{ immediate: true },
			);

			const startEdit = () => {
				draft.value = props.section.inner;
				editing.value = true;
			};

			const save = () => {
				emit('save', {
					id: props.section.id,
					markdown: draft.value,
					done: (ok: boolean) => {
						if (ok) editing.value = false;
					},
				});
			};

			return {
				editing,
				draft,
				author,
				label,
				readOnly,
				ownerIcon,
				startEdit,
				save,
			};
		},
	});
</script>
