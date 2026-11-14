<template>
	<div
		class="rounded-lg border border-line bg-surface p-4"
		data-testid="memory-draft"
	>
		<template v-if="editing">
			<Textarea
				v-model="editText"
				:maxlength="MAX_FACT_LENGTH"
				rows="3"
				:aria-label="$t('workspace.workspaceMemory.editFact')"
			/>
			<div class="mt-2 flex gap-2">
				<Button
					size="sm"
					:disabled="busy || !editText.trim()"
					@click="saveEdit"
				>
					{{ $t('workspace.workspaceMemory.save') }}
				</Button>
				<Button variant="outline" size="sm" @click="editing = false">
					{{ $t('workspace.workspaceMemory.cancel') }}
				</Button>
			</div>
		</template>
		<template v-else>
			<template v-if="proposal.kind === 'add'">
				<p class="text-sm [overflow-wrap:anywhere] text-ink">
					{{ proposal.fact }}
				</p>
			</template>
			<template v-else>
				<div class="mb-1 text-xs font-medium text-ink-subtle uppercase">
					{{
						proposal.kind === 'replace'
							? $t('workspace.workspaceMemory.summarize.kindReplace')
							: $t('workspace.workspaceMemory.summarize.kindDelete')
					}}
				</div>
				<p
					class="text-sm [overflow-wrap:anywhere] text-ink-subtle line-through"
					data-testid="draft-old"
				>
					{{ proposal.target_text }}
				</p>
				<p
					v-if="proposal.kind === 'replace'"
					class="mt-1 text-sm [overflow-wrap:anywhere] text-ink"
					data-testid="draft-new"
				>
					{{ proposal.fact }}
				</p>
				<p
					v-else-if="proposal.fact"
					class="mt-1 text-xs text-ink-subtle"
					data-testid="draft-reason"
				>
					{{
						$t('workspace.workspaceMemory.summarize.reason', {
							reason: proposal.fact,
						})
					}}
				</p>
			</template>

			<div class="mt-2 text-xs text-ink-subtle">
				<template v-if="proposal.task_id">
					<i18n-t keypath="workspace.workspaceMemory.source" tag="span">
						<template #source>
							<router-link
								:to="`/${workspaceCode}/tasks/${proposal.task_id}`"
								class="underline"
							>
								{{
									proposal.source ||
									$t('workspace.workspaceMemory.taskRef', {
										id: proposal.task_id,
									})
								}}
							</router-link>
						</template>
					</i18n-t>
				</template>
				<template v-else-if="proposal.source">
					{{
						$t('workspace.workspaceMemory.source', {
							source: proposal.source,
						})
					}}
				</template>
			</div>

			<p
				v-if="entryChanged"
				class="mt-2 text-xs font-medium text-destructive"
				data-testid="draft-entry-changed"
			>
				{{ $t('workspace.workspaceMemory.summarize.entryChangedRefresh') }}
			</p>

			<div class="mt-3 flex flex-wrap gap-2">
				<Button v-if="entryChanged" size="sm" @click="$emit('changed')">
					{{ $t('workspace.workspaceMemory.summarize.refresh') }}
				</Button>
				<template v-else>
					<Button size="sm" :disabled="busy" @click="apply">
						{{ applyLabel }}
					</Button>
					<Button
						v-if="proposal.kind !== 'delete'"
						variant="outline"
						size="sm"
						:disabled="busy"
						@click="startEdit"
					>
						{{ $t('workspace.workspaceMemory.edit') }}
					</Button>
				</template>
				<Button variant="outline" size="sm" :disabled="busy" @click="skip">
					{{ $t('workspace.workspaceMemory.summarize.skip') }}
				</Button>
			</div>
		</template>
	</div>
</template>

<script lang="ts">
	import {
		acceptMemoryProposal,
		memoryErrorMessage,
		rejectMemoryProposal,
		undoMemoryProposal,
		updateMemoryProposal,
		type MemoryProposal,
	} from '@/actions/tmgr/memory';
	import { Button } from '@/components/ui/button';
	import { Textarea } from '@/components/ui/textarea';
	import { ToastAction, useToast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import { MAX_FACT_LENGTH, UNDO_WINDOW_MS } from '@/utils/memory';
	import { computed, defineComponent, h, ref, type PropType } from 'vue';

	const prefix = 'workspace.workspaceMemory.';

	export default defineComponent({
		name: 'SummarizeDraftCard',
		components: { Button, Textarea },
		props: {
			proposal: { type: Object as PropType<MemoryProposal>, required: true },
			workspaceId: { type: Number, required: true },
			workspaceCode: { type: String, default: '' },
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const { toast } = useToast();

			const busy = ref(false);
			const editing = ref(false);
			const editText = ref('');
			const entryChanged = ref(false);

			const applyLabel = computed(() =>
				t(
					props.proposal.kind === 'add'
						? `${prefix}add`
						: props.proposal.kind === 'replace'
							? `${prefix}summarize.apply`
							: `${prefix}delete`,
				),
			);

			const notify = (title: string, destructive = false) =>
				toast({ title, variant: destructive ? 'destructive' : 'default' });

			const run = async (action: () => Promise<void>) => {
				busy.value = true;
				try {
					await action();
				} finally {
					busy.value = false;
				}
			};

			const undo = async () => {
				try {
					await undoMemoryProposal(props.workspaceId, props.proposal.id);
				} catch (error: any) {
					if (error?.response?.status === 409)
						notify(t(`${prefix}summarize.alreadyChanged`));
					else
						notify(
							memoryErrorMessage(error, t(`${prefix}restoreFailed`)),
							true,
						);
				}
				emit('changed');
			};

			const appliedTitle = () =>
				props.proposal.kind === 'add'
					? t(`${prefix}summarize.added`)
					: props.proposal.kind === 'replace'
						? t(`${prefix}summarize.replaced`)
						: t(`${prefix}deleted`);

			const apply = () =>
				run(async () => {
					try {
						await acceptMemoryProposal(props.workspaceId, props.proposal.id);
					} catch (error: any) {
						const data = error?.response?.data;
						if (
							error?.response?.status === 409 &&
							data?.error === 'entry_changed'
						) {
							entryChanged.value = true;
						} else {
							notify(memoryErrorMessage(error, t(`${prefix}addFailed`)), true);
						}
						return;
					}
					toast({
						title: appliedTitle(),
						duration: UNDO_WINDOW_MS,
						action: h(
							ToastAction,
							{ altText: t(`${prefix}undo`), onClick: undo },
							() => t(`${prefix}undo`),
						),
					});
					emit('changed');
				});

			const skip = () =>
				run(async () => {
					try {
						const result = await rejectMemoryProposal(
							props.workspaceId,
							props.proposal.id,
						);
						if (result.outcome === 'already_added')
							notify(t(`${prefix}alreadyAdded`));
					} catch (error) {
						notify(memoryErrorMessage(error, t(`${prefix}rejectFailed`)), true);
						return;
					}
					emit('changed');
				});

			const startEdit = () => {
				editText.value = props.proposal.fact;
				editing.value = true;
			};

			const saveEdit = () =>
				run(async () => {
					try {
						await updateMemoryProposal(props.workspaceId, props.proposal.id, {
							fact: editText.value.trim(),
						});
						editing.value = false;
					} catch (error) {
						notify(memoryErrorMessage(error, t(`${prefix}saveFailed`)), true);
						return;
					}
					emit('changed');
				});

			return {
				MAX_FACT_LENGTH,
				apply,
				applyLabel,
				busy,
				editText,
				editing,
				entryChanged,
				saveEdit,
				skip,
				startEdit,
			};
		},
	});
</script>
