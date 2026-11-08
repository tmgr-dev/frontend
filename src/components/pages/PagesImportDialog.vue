<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent
			class="max-h-[90vh] max-w-2xl overflow-y-auto"
			data-testid="pages-import-dialog"
		>
			<DialogHeader>
				<DialogTitle>{{ $t('pages.import.title') }}</DialogTitle>
				<DialogDescription data-testid="pages-import-target">
					{{
						$t('pages.import.target', {
							title: parentTitle || $t('pages.import.root'),
						})
					}}
				</DialogDescription>
			</DialogHeader>

			<div v-if="flow.step.value === 'pick'" class="space-y-3">
				<div
					class="flex flex-col items-center gap-2 rounded-md border-2 border-dashed px-4 py-8 text-center text-sm text-ink-subtle"
					:class="
						zone.dragging.value ? 'border-brand bg-brand-bg' : 'border-line'
					"
					data-testid="pages-import-dropzone"
					v-on="zone.handlers"
				>
					<Upload class="h-6 w-6" />
					<p>{{ $t('pages.import.drop') }}</p>
					<Button
						variant="outline"
						type="button"
						data-testid="pages-import-choose"
						@click="input?.click()"
					>
						{{ $t('pages.import.choose') }}
					</Button>
					<input
						ref="input"
						type="file"
						multiple
						class="hidden"
						accept=".md,.markdown,.zip,image/*,.pdf,.txt"
						data-testid="pages-import-input"
						@change="onPick"
					/>
				</div>
				<p
					v-if="flow.error.value"
					class="text-sm text-status-fix-fg"
					role="alert"
					data-testid="pages-import-error"
				>
					{{ flow.error.value }}
				</p>
			</div>

			<div
				v-else-if="flow.step.value === 'reading'"
				class="flex items-center gap-2 py-8 text-sm text-ink-subtle"
				data-testid="pages-import-reading"
			>
				<Loader2 class="h-4 w-4 animate-spin" />
				{{ $t('pages.import.reading') }}
			</div>

			<div
				v-else-if="flow.step.value === 'preview'"
				class="space-y-4"
				data-testid="pages-import-preview"
			>
				<p class="text-sm text-ink" data-testid="pages-import-counts">
					{{
						flow.conflictCount.value
							? $t(
									'pages.import.willCreateConflicts',
									{
										n: flow.importCount.value,
										conflicts: flow.conflictCount.value,
									},
									flow.importCount.value,
								)
							: $t(
									'pages.import.willCreate',
									{ n: flow.importCount.value },
									flow.importCount.value,
								)
					}}.
				</p>

				<ul
					class="max-h-72 overflow-y-auto rounded-md border border-line bg-surface py-1 text-sm"
					data-testid="pages-import-tree"
				>
					<li
						v-for="row in flow.rows.value"
						:key="row.page.key"
						class="flex items-center gap-2 py-1 pr-3"
						:class="row.skipped ? 'opacity-50' : ''"
						:style="{ paddingLeft: `${12 + row.depth * 16}px` }"
						data-testid="pages-import-row"
					>
						<span class="min-w-0 flex-1 truncate text-ink">{{
							row.page.title
						}}</span>
						<span
							v-if="row.page.type !== 'plain'"
							class="shrink-0 rounded bg-tag-bg px-1.5 py-0.5 text-xs text-tag-fg"
							>{{ row.page.type }}</span
						>
						<span
							v-if="row.page.conflict"
							class="shrink-0 rounded bg-status-progress-bg px-1.5 py-0.5 text-xs text-status-progress-fg"
							data-testid="pages-import-conflict-badge"
							>{{
								row.skipped
									? $t('pages.import.badgeSkipped')
									: $t('pages.import.badgeExists')
							}}</span
						>
					</li>
				</ul>

				<label
					v-if="flow.hasConflicts.value"
					class="flex flex-col gap-1 text-sm text-ink"
				>
					{{ $t('pages.import.conflictPolicy') }}
					<select
						v-model="flow.policy.value"
						class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-hidden"
						data-testid="pages-import-policy"
					>
						<option value="rename">
							{{ $t('pages.import.policyRename') }}
						</option>
						<option value="skip">{{ $t('common.actions.skip') }}</option>
						<option value="import">
							{{ $t('pages.import.policyImport') }}
						</option>
					</select>
				</label>

				<div
					v-if="flow.warnings.value.length"
					class="rounded-md border border-status-progress bg-status-progress-bg p-3 text-sm text-status-progress-fg"
					data-testid="pages-import-warnings"
				>
					<button
						type="button"
						class="flex w-full items-center gap-1 text-left font-medium"
						:aria-expanded="warningsShown"
						@click="warningsOpen = !warningsOpen"
					>
						<ChevronRight
							class="h-4 w-4 transition-transform"
							:class="warningsShown ? 'rotate-90' : ''"
						/>
						{{
							$t(
								'pages.import.warnings',
								{ n: flow.warnings.value.length },
								flow.warnings.value.length,
							)
						}}
					</button>
					<ul
						v-if="warningsShown"
						class="space-y-1 mt-2 max-h-40 overflow-y-auto"
					>
						<li v-for="(warning, index) in flow.warnings.value" :key="index">
							<span v-if="warning.path" class="font-mono text-xs"
								>{{ warning.path }}: </span
							>{{ warning.message }}
						</li>
					</ul>
				</div>
			</div>

			<div
				v-else-if="flow.step.value === 'running'"
				class="space-y-2 py-4"
				data-testid="pages-import-progress"
			>
				<div class="h-2 overflow-hidden rounded-full bg-line-strong">
					<div
						class="h-full bg-primary transition-all"
						:style="{ width: `${percent}%` }"
					></div>
				</div>
				<p class="text-sm text-ink">
					{{ flow.progress.value.done }} / {{ flow.progress.value.total }}
				</p>
				<p class="truncate text-xs text-ink-subtle">
					{{ flow.progress.value.current }}
				</p>
			</div>

			<div v-else class="space-y-3" data-testid="pages-import-summary">
				<p
					v-if="flow.result.value?.error"
					class="rounded-md border border-status-fix bg-status-fix-bg p-3 text-sm text-status-fix-fg"
					role="alert"
					data-testid="pages-import-run-error"
				>
					{{
						$t(
							'pages.import.stopped',
							{ error: flow.result.value.error, n: created.length },
							created.length,
						)
					}}
				</p>
				<p v-else class="text-sm text-ink">
					{{
						$t('pages.import.created', { n: created.length }, created.length)
					}}
				</p>
				<p
					v-if="flow.incompleteMessage.value"
					class="rounded-md border border-status-fix bg-status-fix-bg p-3 text-sm text-status-fix-fg"
					role="alert"
					data-testid="pages-import-incomplete"
				>
					{{ flow.incompleteMessage.value }}
				</p>
				<p v-if="skipped.length" class="text-sm text-ink-subtle">
					{{ $t('pages.import.skippedCount', { n: skipped.length }) }}
				</p>
				<ul
					v-if="created.length"
					class="space-y-1 max-h-60 overflow-y-auto text-sm"
				>
					<li v-for="page in created" :key="page.id">
						<router-link
							:to="`/${workspaceCode}/pages/${page.slug}`"
							class="text-brand underline"
							data-testid="pages-import-created-link"
							@click="close"
							>{{ page.title }}</router-link
						>
					</li>
				</ul>
				<ul
					v-if="resultWarnings.length"
					class="space-y-1 max-h-40 overflow-y-auto rounded-md border border-status-progress bg-status-progress-bg p-3 text-sm text-status-progress-fg"
				>
					<li v-for="(warning, index) in resultWarnings" :key="index">
						<span v-if="warning.path" class="font-mono text-xs"
							>{{ warning.path }}: </span
						>{{ warning.message }}
					</li>
				</ul>
			</div>

			<DialogFooter class="gap-2 sm:gap-2">
				<template v-if="flow.step.value === 'done'">
					<Button data-testid="pages-import-close" @click="close">
						{{ $t('common.actions.close') }}
					</Button>
				</template>
				<template v-else>
					<Button
						variant="outline"
						:disabled="flow.step.value === 'running'"
						data-testid="pages-import-cancel"
						@click="close"
					>
						{{ $t('common.actions.cancel') }}
					</Button>
					<Button
						v-if="flow.step.value === 'preview'"
						data-testid="pages-import-submit"
						:disabled="flow.importCount.value === 0"
						@click="flow.submit"
					>
						{{
							$t(
								'pages.import.submit',
								{ n: flow.importCount.value },
								flow.importCount.value,
							)
						}}
					</Button>
				</template>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { usePagesImportFlow } from '@/composable/usePagesImportFlow';
	import { useFileDrop } from '@/composable/usePagesMarkdownIo';
	import { ChevronRight, Loader2, Upload } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onMounted,
		ref,
		type PropType,
	} from 'vue';

	export default defineComponent({
		name: 'PagesImportDialog',
		components: {
			Button,
			ChevronRight,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Loader2,
			Upload,
		},
		props: {
			open: { type: Boolean, default: true },
			files: { type: Array as PropType<File[]>, default: () => [] },
			parentId: { type: Number as PropType<number | null>, default: null },
			parentTitle: { type: String as PropType<string | null>, default: null },
			workspaceCode: { type: String, required: true },
		},
		emits: ['update:open'],
		setup(props, { emit }) {
			const flow = usePagesImportFlow(() => ({
				parentId: props.parentId,
				workspaceCode: props.workspaceCode,
			}));
			const input = ref<HTMLInputElement | null>(null);
			const warningsOpen = ref(false);
			const zone = useFileDrop((files) => void flow.start(files));

			const close = () => emit('update:open', false);
			const onOpenChange = (open: boolean) => {
				if (!open && flow.step.value !== 'running') close();
			};
			const onPick = (event: Event) => {
				const target = event.target as HTMLInputElement;
				const files = Array.from(target.files ?? []);
				target.value = '';
				if (files.length) void flow.start(files);
			};

			onMounted(() => {
				if (props.files.length) {
					void flow.start(props.files);
				}
			});

			const percent = computed(() => {
				const { done, total } = flow.progress.value;
				return total ? Math.min(100, Math.round((done / total) * 100)) : 0;
			});
			const warningsShown = computed(
				() => flow.warnings.value.length <= 3 || warningsOpen.value,
			);
			const created = computed(() => flow.result.value?.created ?? []);
			const skipped = computed(() => flow.result.value?.skipped ?? []);
			const resultWarnings = computed(() => flow.result.value?.warnings ?? []);

			return {
				flow,
				input,
				warningsOpen,
				warningsShown,
				zone,
				close,
				onOpenChange,
				onPick,
				percent,
				created,
				skipped,
				resultWarnings,
			};
		},
	});
</script>
