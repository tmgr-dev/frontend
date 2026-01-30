<template>
	<div class="task-relations">
		<!-- Add link button (hidden when hideAddButton is true) -->
		<button
			v-if="!hideAddButton"
			@click="isAddingRelation = true"
			class="mb-2 flex items-center gap-1 text-xs text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 transition-colors"
		>
			<svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
				<path fill-rule="evenodd" d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z" clip-rule="evenodd" />
			</svg>
			<span>Add link</span>
		</button>

		<!-- Content - only show when there are relations -->
		<div v-if="relations && relations.length > 0">
			<!-- Grouped Relations -->
			<div class="space-y-3">
				<div v-for="(group, type) in groupedRelations" :key="type" class="space-y-1">
					<div class="flex items-center gap-1">
						<component :is="getRelationIcon(type)" class="h-3 w-3 text-gray-500" />
						<h4 class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase">
							{{ type }}
						</h4>
					</div>
					
					<div class="space-y-0.5">
						<div
							v-for="relation in group"
							:key="relation.id"
							class="group flex items-center justify-between rounded border dark:border-gray-700 bg-white dark:bg-gray-900 px-2 py-1.5 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
						>
							<div 
								class="flex items-center gap-2 flex-1 min-w-0 cursor-pointer"
								@click="emit('openTask', relation.related_task.id)"
							>
								<span class="text-xs font-mono text-gray-500 dark:text-gray-400">
									#{{ relation.related_task.id }}
								</span>
								<span 
									class="text-xs text-gray-900 dark:text-gray-100 hover:text-blue-600 dark:hover:text-blue-400"
									:title="relation.related_task.title"
								>
									{{ truncateTitle(relation.related_task.title) }}
								</span>
							</div>
							<button
								@click="handleRemoveRelation(relation)"
								class="opacity-0 group-hover:opacity-100 inline-flex items-center justify-center rounded transition-colors hover:bg-red-50 dark:hover:bg-red-900/20 hover:text-red-600 dark:hover:text-red-400 h-5 w-5 ml-1"
							>
								<svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
									<path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
								</svg>
							</button>
						</div>
					</div>
				</div>
			</div>
		</div>

		<div v-else-if="!hideAddButton" class="text-sm text-gray-500 dark:text-gray-400">
			No linked tasks yet
		</div>

		<!-- Compact Modal -->
		<div v-if="isAddingRelation" class="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm" @click="cancelAdding">
		<div 
			class="fixed left-[50%] top-[50%] z-50 grid w-full max-w-sm translate-x-[-50%] translate-y-[-50%] gap-2 border dark:border-gray-700 bg-white dark:bg-gray-900 p-3 shadow-lg rounded-lg"
			@click.stop
		>
			<!-- Dialog Header -->
			<div class="flex items-center justify-between">
				<h2 class="text-sm font-semibold">Link Issue</h2>
				<button
					@click="cancelAdding"
					class="rounded opacity-70 hover:opacity-100"
				>
					<svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
						<path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
					</svg>
				</button>
			</div>

			<!-- Dialog Content -->
			<div class="grid gap-2">
				<!-- Task Search -->
				<div class="grid gap-1">
					<label class="text-xs font-medium">Issue</label>
					<div class="relative">
						<svg xmlns="http://www.w3.org/2000/svg" class="absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-gray-400" viewBox="0 0 20 20" fill="currentColor">
							<path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd" />
						</svg>
						<input
							v-model="taskSearchQuery"
							type="text"
							placeholder="Search..."
							class="flex h-7 w-full rounded border dark:border-gray-700 bg-white dark:bg-gray-800 px-2 pl-7 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-blue-500"
							@input="handleTaskSearch((($event.target as HTMLInputElement)?.value || ''))"
						/>
					</div>
					
					<div v-if="taskSearchQuery && (searchResults.length > 0 || isSearching)" class="rounded border dark:border-gray-700 bg-white dark:bg-gray-800 shadow-md max-h-40 overflow-auto">
						<div v-if="isSearching" class="py-4 text-center text-xs text-gray-500">
							Searching...
						</div>
						<button
							v-for="task in searchResults"
							:key="task.id"
							@click="selectTask(task)"
							:class="[
								'relative flex w-full items-center px-2 py-1 text-xs outline-none hover:bg-gray-100 dark:hover:bg-gray-700',
								selectedTask?.id === task.id ? 'bg-gray-100 dark:bg-gray-700' : ''
							]"
						>
							<span class="truncate text-gray-900 dark:text-gray-100">{{ task.title }}</span>
						</button>
					</div>
				</div>

				<!-- Relationship Type -->
				<div class="grid gap-1">
					<label class="text-xs font-medium">Type</label>
					<div class="relative">
						<select
							v-model="newRelation.typeId"
							class="flex h-7 w-full rounded border dark:border-gray-700 bg-white dark:bg-gray-800 px-2 text-xs focus:outline-none focus:ring-1 focus:ring-blue-500 appearance-none"
						>
							<option value="">Select...</option>
							<option
								v-for="type in relationTypes"
								:key="type.id"
								:value="type.id"
							>
								{{ type.name }}
							</option>
						</select>
						<svg xmlns="http://www.w3.org/2000/svg" class="absolute right-2 top-1/2 h-3 w-3 -translate-y-1/2 opacity-50 pointer-events-none" viewBox="0 0 20 20" fill="currentColor">
							<path fill-rule="evenodd" d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" clip-rule="evenodd" />
						</svg>
					</div>
				</div>

				<!-- Preview -->
				<div v-if="selectedTask && selectedRelationType" class="rounded border dark:border-gray-700 bg-gray-50 dark:bg-gray-800 px-2 py-1.5">
					<p class="text-xs text-gray-500 dark:text-gray-400">
						<span class="font-medium text-gray-900 dark:text-gray-100">{{ selectedRelationType }}</span>
						<span class="font-mono">#{{ selectedTask.id }}</span>
					</p>
				</div>

				<!-- Error Message -->
				<div v-if="error" class="rounded border border-red-200 dark:border-red-800 bg-red-50 dark:bg-red-900/20 px-2 py-1.5">
					<p class="text-xs text-red-700 dark:text-red-400">{{ error }}</p>
				</div>
			</div>

			<!-- Dialog Footer -->
			<div class="flex justify-end gap-1.5">
				<button
					@click="cancelAdding"
					class="inline-flex items-center justify-center rounded text-xs font-medium transition-colors border dark:border-gray-700 bg-white dark:bg-gray-800 hover:bg-gray-100 dark:hover:bg-gray-700 h-6 px-2"
				>
					Cancel
				</button>
				<button
					@click="handleAddRelation"
					:disabled="!selectedTask || !newRelation.typeId || isLoading"
					class="inline-flex items-center justify-center rounded text-xs font-medium transition-colors disabled:pointer-events-none disabled:opacity-50 bg-blue-600 text-white hover:bg-blue-700 h-6 px-2"
				>
					{{ isLoading ? 'Linking...' : 'Link' }}
				</button>
			</div>
		</div>
		</div>

		<!-- Confirm Dialog -->
		<Confirm
			v-if="showRemoveConfirm"
			title="Remove link"
			:body="`Remove link '${relationToRemove?.relation_type.name}' to task #${relationToRemove?.related_task.id}?`"
			@on-ok="confirmRemoveRelation"
			@on-cancel="cancelRemoveRelation"
		/>
	</div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, h } from 'vue';
import Confirm from '@/components/general/Confirm.vue';
import {
	getTaskRelationTypes,
	addTaskRelation,
	removeTaskRelation,
	searchTasks,
	type TaskRelationType,
	type TaskRelation,
	type RelatedTask
} from '@/actions/tmgr/taskRelations';

const props = defineProps<{
	taskId: number;
	relations?: TaskRelation[];
	hideAddButton?: boolean;
}>();

const emit = defineEmits<{
	(e: 'update'): void;
	(e: 'openTask', taskId: number): void;
}>();

const isAddingRelation = ref(false);
const isLoading = ref(false);
const isSearching = ref(false);
const error = ref<string | null>(null);
const showRemoveConfirm = ref(false);
const relationToRemove = ref<TaskRelation | null>(null);
const relationTypes = ref<TaskRelationType[]>([]);
const newRelation = ref({
	taskId: null as number | null,
	typeId: '' as string | number
});
const taskSearchQuery = ref('');
const searchResults = ref<RelatedTask[]>([]);
const selectedTask = ref<RelatedTask | null>(null);

let searchTimeout: number | null = null;


const groupedRelations = computed(() => {
	if (!props.relations || props.relations.length === 0) {
		return {};
	}
	
	const grouped: Record<string, TaskRelation[]> = {};
	props.relations.forEach((relation) => {
		const type = relation.relation_type.name;
		if (!grouped[type]) {
			grouped[type] = [];
		}
		grouped[type].push(relation);
	});
	
	return grouped;
});

const selectedRelationType = computed(() => {
	if (!newRelation.value.typeId) return '';
	const type = relationTypes.value.find(t => t.id === Number(newRelation.value.typeId));
	return type ? type.name : '';
});

const truncateTitle = (title: string, maxLength: number = 60): string => {
	if (!title) return '';
	if (title.length <= maxLength) return title;
	return title.substring(0, maxLength) + '...';
};

const getRelationIcon = (type: string) => {
	const iconMap: Record<string, any> = {
		'blocks': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', viewBox: '0 0 20 20', fill: 'currentColor' }, [
			h('path', { 'fill-rule': 'evenodd', d: 'M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z', 'clip-rule': 'evenodd' })
		]),
		'is blocked by': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', viewBox: '0 0 20 20', fill: 'currentColor' }, [
			h('path', { 'fill-rule': 'evenodd', d: 'M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z', 'clip-rule': 'evenodd' })
		]),
		'relates to': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', fill: 'none', viewBox: '0 0 24 24', stroke: 'currentColor' }, [
			h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'stroke-width': '2', d: 'M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1' })
		]),
		'duplicates': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', viewBox: '0 0 20 20', fill: 'currentColor' }, [
			h('path', { d: 'M7 9a2 2 0 012-2h6a2 2 0 012 2v6a2 2 0 01-2 2H9a2 2 0 01-2-2V9z' }),
			h('path', { d: 'M5 3a2 2 0 00-2 2v6a2 2 0 002 2V5h8a2 2 0 00-2-2H5z' })
		]),
		'is duplicated by': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', viewBox: '0 0 20 20', fill: 'currentColor' }, [
			h('path', { d: 'M7 9a2 2 0 012-2h6a2 2 0 012 2v6a2 2 0 01-2 2H9a2 2 0 01-2-2V9z' }),
			h('path', { d: 'M5 3a2 2 0 00-2 2v6a2 2 0 002 2V5h8a2 2 0 00-2-2H5z' })
		]),
		'depends on': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', viewBox: '0 0 20 20', fill: 'currentColor' }, [
			h('path', { 'fill-rule': 'evenodd', d: 'M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z', 'clip-rule': 'evenodd' }),
			h('path', { 'fill-rule': 'evenodd', d: 'M3 10a1 1 0 011-1h8a1 1 0 110 2H4a1 1 0 01-1-1z', 'clip-rule': 'evenodd' })
		]),
		'is dependency of': () => h('svg', { xmlns: 'http://www.w3.org/2000/svg', class: 'h-4 w-4', viewBox: '0 0 20 20', fill: 'currentColor' }, [
			h('path', { 'fill-rule': 'evenodd', d: 'M12.707 5.293a1 1 0 010 1.414L9.414 10l3.293 3.293a1 1 0 01-1.414 1.414l-4-4a1 1 0 010-1.414l4-4a1 1 0 011.414 0z', 'clip-rule': 'evenodd' }),
			h('path', { 'fill-rule': 'evenodd', d: 'M17 10a1 1 0 01-1 1H8a1 1 0 110-2h8a1 1 0 011 1z', 'clip-rule': 'evenodd' })
		]),
	};
	
	return iconMap[type] || iconMap['relates to'];
};

const loadRelationTypes = async () => {
	try {
		relationTypes.value = await getTaskRelationTypes();
	} catch (err: any) {
		console.error('Failed to load relation types:', err);
		error.value = 'Failed to load relation types';
	}
};

const handleTaskSearch = async (query: string) => {
	if (searchTimeout) {
		clearTimeout(searchTimeout);
	}

	if (!query || query.trim().length < 2) {
		searchResults.value = [];
		return;
	}

	searchTimeout = window.setTimeout(async () => {
		isSearching.value = true;
		try {
			const results = await searchTasks(query.trim());
			searchResults.value = results.filter(task => task.id !== props.taskId);
		} catch (err: any) {
			console.error('Failed to search tasks:', err);
			searchResults.value = [];
		} finally {
			isSearching.value = false;
		}
	}, 300);
};

const selectTask = (task: RelatedTask) => {
	selectedTask.value = task;
	newRelation.value.taskId = task.id;
	taskSearchQuery.value = task.title;
	searchResults.value = [];
};

const handleAddRelation = async () => {
	if (!selectedTask.value || !newRelation.value.typeId) {
		error.value = 'Please fill all fields';
		return;
	}

	isLoading.value = true;
	error.value = null;

	try {
		await addTaskRelation(
			props.taskId,
			selectedTask.value.id,
			Number(newRelation.value.typeId)
		);
		emit('update');
		cancelAdding();
	} catch (err: any) {
		error.value = err.response?.data?.message || 'Failed to add task relation';
		console.error('Failed to add relation:', err);
	} finally {
		isLoading.value = false;
	}
};

const handleRemoveRelation = (relation: TaskRelation) => {
	relationToRemove.value = relation;
	showRemoveConfirm.value = true;
};

const confirmRemoveRelation = async () => {
	if (!relationToRemove.value) return;

	isLoading.value = true;
	error.value = null;

	try {
		await removeTaskRelation(
			props.taskId,
			relationToRemove.value.related_task.id,
			relationToRemove.value.relation_type.id
		);
		emit('update');
	} catch (err: any) {
		error.value = err.response?.data?.message || 'Failed to remove task relation';
		console.error('Failed to remove relation:', err);
	} finally {
		isLoading.value = false;
		showRemoveConfirm.value = false;
		relationToRemove.value = null;
	}
};

const cancelRemoveRelation = () => {
	showRemoveConfirm.value = false;
	relationToRemove.value = null;
};

const cancelAdding = () => {
	isAddingRelation.value = false;
	newRelation.value = {
		taskId: null,
		typeId: ''
	};
	selectedTask.value = null;
	taskSearchQuery.value = '';
	searchResults.value = [];
	error.value = null;
};

onMounted(() => {
	loadRelationTypes();
});

onUnmounted(() => {
	if (searchTimeout) {
		clearTimeout(searchTimeout);
	}
});

const openAddModal = () => {
	isAddingRelation.value = true;
};

defineExpose({
	openAddModal
});
</script>

<style scoped>
/* Modern shadcn-like styling */
</style>

