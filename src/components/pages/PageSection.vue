<template>
	<section
		class="my-4 -ml-4 rounded-lg border-l-[3px] border-status-testing bg-status-testing-bg py-3 pr-4 pl-[13px]"
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
					class="text-status-testing-fg"
				/>
				<span
					v-else
					class="inline-flex items-center gap-1.5 text-sm font-semibold text-status-testing-fg"
				>
					<component :is="ownerIcon" class="h-4 w-4" />
					{{ label }}
				</span>
				<span
					class="rounded bg-surface-active px-1.5 py-0.5 text-2xs tracking-wide text-status-testing-fg uppercase"
					>{{ section.owner }}</span
				>
			</span>
			<button
				v-if="!editing && !readOnly"
				type="button"
				class="inline-flex items-center gap-1 rounded px-2 py-1 text-xs text-status-testing-fg hover:bg-surface-active"
				@click="requestEdit"
			>
				<Pencil class="h-3.5 w-3.5" />
				{{ $t('common.actions.edit') }}
			</button>
		</header>

		<AlertDialog v-model:open="confirming">
			<AlertDialogContent @close-auto-focus="restoreEditFocus">
				<AlertDialogHeader
					><AlertDialogTitle>{{
						$t('pages.agentSection.title')
					}}</AlertDialogTitle>
					<AlertDialogDescription>{{
						$t('pages.agentSection.description')
					}}</AlertDialogDescription></AlertDialogHeader
				>
				<AlertDialogFooter
					><AlertDialogCancel>{{
						$t('common.actions.cancel')
					}}</AlertDialogCancel>
					<AlertDialogAction @click="startEdit">{{
						$t('pages.agentSection.continue')
					}}</AlertDialogAction></AlertDialogFooter
				>
			</AlertDialogContent>
		</AlertDialog>
		<div v-if="editing">
			<textarea
				ref="textarea"
				v-model="draft"
				rows="8"
				class="w-full rounded-md border border-input bg-background p-2 font-mono text-sm text-ink"
				data-testid="section-textarea"
			></textarea>
			<div class="mt-2 flex justify-end gap-2">
				<button
					type="button"
					class="rounded-md px-3 py-1.5 text-sm text-ink-muted hover:bg-surface-hover"
					:disabled="saving"
					@click="editing = false"
				>
					{{ $t('common.actions.cancel') }}
				</button>
				<button
					type="button"
					class="rounded-md bg-status-testing-fg px-3 py-1.5 text-sm font-medium text-surface-base hover:opacity-90 disabled:opacity-50"
					:disabled="saving"
					@click="save"
				>
					{{ $t('common.actions.save') }}
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
	import {
		AlertDialog,
		AlertDialogContent,
		AlertDialogHeader,
		AlertDialogTitle,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogCancel,
		AlertDialogAction,
	} from '@/components/ui/alert-dialog';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import { ownerAuthorRef, ownerLabel } from '@/utils/pages/author';
	import { parseOwner, type SectionSegment } from '@/utils/pages/sections';
	import { Bot, Cog, Pencil, UserRound } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType, ref, watch } from 'vue';
	import PageMarkdown from './PageMarkdown.vue';
	import type { TmgrDirectory } from './useTmgrDirectory';

	export default defineComponent({
		name: 'PageSection',
		components: {
			AlertDialog,
			AlertDialogContent,
			AlertDialogHeader,
			AlertDialogTitle,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogCancel,
			AlertDialogAction,
			AuthorBadge,
			PageMarkdown,
			Pencil,
		},
		props: {
			section: { type: Object as PropType<SectionSegment>, required: true },
			directory: { type: Object as PropType<TmgrDirectory>, required: true },
			saving: { type: Boolean, default: false },
		},
		emits: ['save', 'navigate'],
		setup(props, { emit }) {
			const textarea = ref<HTMLTextAreaElement | null>(null);
			const restoreEditFocus = (event: Event) => {
				if (textarea.value) {
					event.preventDefault();
					textarea.value.focus();
				}
			};
			const confirming = ref(false);
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

			const requestEdit = () => {
				if (props.section.owner === 'agents') confirming.value = true;
				else startEdit();
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
				textarea,
				restoreEditFocus,
				confirming,
				requestEdit,
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
