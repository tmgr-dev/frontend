<template>
	<Dialog :open="true" @update:open="onOpen">
		<DialogContent class="max-w-md" data-testid="task-from-selection-dialog">
			<DialogHeader>
				<DialogTitle>Make a task</DialogTitle>
				<DialogDescription>
					The text on the page will be replaced with a link to the task.
				</DialogDescription>
			</DialogHeader>

			<blockquote
				class="max-h-24 overflow-y-auto whitespace-pre-wrap break-words rounded-md border-l-4 border-line-strong bg-surface-sunken px-3 py-2 text-sm text-ink-muted"
			>
				{{ text }}
			</blockquote>

			<div class="space-y-3 text-sm">
				<label class="block">
					<span class="mb-1 block text-xs text-ink-subtle">Category</span>
					<select
						v-model="categoryId"
						class="w-full rounded border border-input bg-background px-2 py-1.5 text-ink"
						data-testid="selection-category"
					>
						<option v-for="c in categories" :key="c.id" :value="c.id">
							{{ c.title }}
						</option>
					</select>
				</label>
				<DoneCheckbox
					v-if="simpleMode"
					v-model="markDone"
					class="text-sm"
					:disabled="doneStatusId === null"
					:tooltip="DONE_DISABLED_TOOLTIP"
				/>
				<label v-else class="block">
					<span class="mb-1 block text-xs text-ink-subtle"
						>Status (optional)</span
					>
					<select
						v-model="statusId"
						class="w-full rounded border border-input bg-background px-2 py-1.5 text-ink"
						data-testid="selection-status"
					>
						<option :value="null">Default</option>
						<option v-for="s in statuses" :key="s.id" :value="s.id">
							{{ s.name }}
						</option>
					</select>
				</label>
				<p v-if="error" class="text-xs text-status-fix-fg" role="alert">
					{{ error }}
				</p>
			</div>

			<DialogFooter class="gap-2 sm:gap-2">
				<Button variant="outline" :disabled="busy" @click="$emit('cancel')">
					Cancel
				</Button>
				<Button
					:disabled="busy || categoryId === null"
					data-testid="selection-submit"
					@click="submit"
				>
					Create task
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { type Category, getCategories } from '@/actions/tmgr/categories';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
	import DoneCheckbox from '@/components/tasks/DoneCheckbox.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { useWorkflowMode } from '@/composable/useWorkflowMode';
	import {
		pickDefaultCategory,
		readLastCategory,
	} from '@/utils/pages/taskFromSelection';
	import { DONE_DISABLED_TOOLTIP, doneTarget } from '@/utils/simpleStatus';
	import { computed, defineComponent, onMounted, ref } from 'vue';

	interface StatusOption {
		id: number;
		name: string;
		type?: string | null;
	}

	export default defineComponent({
		name: 'TaskFromSelectionDialog',
		components: {
			DoneCheckbox,
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: {
			text: { type: String, required: true },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['submit', 'cancel'],
		setup(_props, { emit }) {
			const categories = ref<Category[]>([]);
			const statuses = ref<StatusOption[]>([]);
			const categoryId = ref<number | null>(null);
			const statusId = ref<number | null>(null);
			const markDone = ref(false);
			const { simpleMode } = useWorkflowMode();
			const doneStatusId = computed(() => doneTarget(statuses.value));

			onMounted(async () => {
				const [cats, stats] = await Promise.allSettled([
					getCategories(),
					getWorkspaceStatuses(),
				]);
				if (cats.status === 'fulfilled') {
					categories.value = cats.value;
					categoryId.value = pickDefaultCategory(
						cats.value,
						readLastCategory(),
					);
				}
				if (stats.status === 'fulfilled') {
					statuses.value = (stats.value as StatusOption[]) ?? [];
				}
			});

			const submit = () => {
				if (categoryId.value === null) return;
				emit('submit', {
					category_id: categoryId.value,
					status_id:
						simpleMode.value && markDone.value
							? doneStatusId.value
							: statusId.value,
				});
			};
			const onOpen = (open: boolean) => {
				if (!open) emit('cancel');
			};

			return {
				categories,
				statuses,
				categoryId,
				statusId,
				markDone,
				simpleMode,
				doneStatusId,
				DONE_DISABLED_TOOLTIP,
				submit,
				onOpen,
			};
		},
	});
</script>
