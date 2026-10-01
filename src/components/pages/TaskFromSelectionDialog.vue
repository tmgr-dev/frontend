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
				class="max-h-24 overflow-y-auto whitespace-pre-wrap break-words rounded-md border-l-4 border-gray-300 bg-gray-50 px-3 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
			>
				{{ text }}
			</blockquote>

			<div class="space-y-3 text-sm">
				<label class="block">
					<span class="mb-1 block text-xs text-ink-subtle">Category</span>
					<select
						v-model="categoryId"
						class="w-full rounded border border-gray-300 bg-white px-2 py-1.5 text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
						data-testid="selection-category"
					>
						<option v-for="c in categories" :key="c.id" :value="c.id">
							{{ c.title }}
						</option>
					</select>
				</label>
				<label class="block">
					<span class="mb-1 block text-xs text-ink-subtle"
						>Status (optional)</span
					>
					<select
						v-model="statusId"
						class="w-full rounded border border-gray-300 bg-white px-2 py-1.5 text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
						data-testid="selection-status"
					>
						<option :value="null">Default</option>
						<option v-for="s in statuses" :key="s.id" :value="s.id">
							{{ s.name }}
						</option>
					</select>
				</label>
				<p
					v-if="error"
					class="text-xs text-red-600 dark:text-red-400"
					role="alert"
				>
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
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		pickDefaultCategory,
		readLastCategory,
	} from '@/utils/pages/taskFromSelection';
	import { defineComponent, onMounted, ref } from 'vue';

	interface StatusOption {
		id: number;
		name: string;
	}

	export default defineComponent({
		name: 'TaskFromSelectionDialog',
		components: {
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
					status_id: statusId.value,
				});
			};
			const onOpen = (open: boolean) => {
				if (!open) emit('cancel');
			};

			return { categories, statuses, categoryId, statusId, submit, onOpen };
		},
	});
</script>
