<script setup>
	import Breadcrumbs from '../components/general/Breadcrumbs.vue';
	import extractParents from '../utils/extractParents';
	import getBreadcrumbs from '../utils/getBreadcrumbs';
	import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
	import { getTasks, updateTaskPartially } from '@/actions/tmgr/tasks';
	import {
		restoreCategory as restoreCategoryAction,
		deleteCategory as deleteCategoryAction,
		getParentCategory,
		getSubCategories,
	} from '@/actions/tmgr/categories';
	import Select from '@/components/general/Select.vue';
	import { ref, computed, watch, onMounted } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import store from '@/store';
	import { Button } from '@/components/ui/button';
	import EmptyState from '@/components/EmptyState.vue';
	import {
		ClipboardListIcon,
		CircleUserRoundIcon,
		FolderClosedIcon,
		FolderPlusIcon,
		FolderPenIcon,
	} from 'lucide-vue-next';

	const route = useRoute();
	const router = useRouter();
	const item = ref(true);
	const tasks = ref(null);
	const categories = ref(null);
	const category = ref(null);
	const parentCategories = ref([]);
	const isCategoriesFirstLoading = ref(true);
	const isTasksFirstLoading = ref(true);
	const isLoadingActions = ref({});
	const loadingActionTasksIds = ref([]);
	const workspaceStatus = ref('all');

	const workspaceStatuses = computed(() => {
		let statuses = store.state.workspaceStatuses;
		return [
			{
				name: 'All',
				id: 'all',
			},
		].concat(statuses);
	});

	const id = computed(() => route.params.id || '');
	const status = computed(() => route.params.status || '');

	watch(workspaceStatus, (newStatus, prevStatus) => {
		if (newStatus === prevStatus) {
			return;
		}

		if (id.value) {
			router.push(
				`/projects-categories/${
					id.value ? `${id.value}/children` : '/status'
				}/${newStatus}`,
			);
		} else {
			router.push({
				name: 'ProjectCategoryChildrenListWithStatus',
				params: { status: newStatus },
			});
		}
	});

	function setLoadingActions(tasks) {
		tasks.forEach((task) => {
			isLoadingActions.value[`hide-${task.id}`] = false;
			isLoadingActions.value[`done-${task.id}`] = false;
			isLoadingActions.value[`start-${task.id}`] = false;
			isLoadingActions.value[`stop-${task.id}`] = false;
			isLoadingActions.value[`activate-${task.id}`] = false;
			isLoadingActions.value[`delete-${task.id}`] = false;
		});
	}

	const drop = async (event, category) => {
		const taskId = +event.dataTransfer.getData('task-id');
		const categoryId = category.id;

		category.hoverClass = '';
		loadingActionTasksIds.value.push(taskId);

		await updateTaskPartially(taskId, {
			project_category_id: categoryId,
		});

		await loadCategories();
		await loadTasks();
		loadingActionTasksIds.value = loadingActionTasksIds.value.filter(
			(id) => id !== taskId,
		);
	};

	const loadTasks = async () => {
		tasks.value = await getTasks(
			{
				params: {
					project_category_id: id.value,
					status_id: status.value || null,
				},
			},
			false,
		);

		setLoadingActions(tasks.value);
		isTasksFirstLoading.value = false;
	};

	const loadParentCategory = async () => {
		category.value = await getParentCategory(id.value);
		store.commit('setMetaTitle', category.value.title || 'Categories');
		parentCategories.value = extractParents(category.value);
	};

	const loadCategories = async () => {
		categories.value = await getSubCategories(id.value, {
			params: {
				all: '',
			},
		});

		if (id.value) {
			await loadParentCategory();
		}

		isCategoriesFirstLoading.value = false;
	};

	const deleteCategory = async (category) => {
		try {
			category.deleted_at = await deleteCategoryAction(category.id);
			await loadTasks();
		} catch (e) {
			console.error(e);
		}
	};

	const restoreCategory = async (category) => {
		try {
			category.deleted_at = await restoreCategoryAction(category.id);
			await loadTasks();
		} catch (e) {
			console.error(e);
		}
	};

	onMounted(async () => {
		await loadCategories();
		await loadTasks();

		if (status.value) {
			workspaceStatus.value =
				status.value === 'all' ? status.value : parseInt(status.value);
		}
	});
</script>

<template>
	<teleport to="title">
		{{ category ? category.title : 'Categories' }}
	</teleport>

	<BaseLayout>
		<template #action>
			<div class="flex items-center justify-between gap-4">
				<breadcrumbs
					:current="category ? category.title : ''"
					:drop="drop"
					:items="getBreadcrumbs(parentCategories)"
				/>

				<div class="flex gap-3">
					<Button
						v-if="category"
						:title="`Edit ${category.title} category`"
						variant="outline"
						@click="() => router.push(`/projects-categories/${category.id}`)"
					>
						<FolderPenIcon />
					</Button>

					<Button
						@click="
							() => router.push(`/projects-categories/${id && `${id}/`}create`)
						"
					>
						<FolderPlusIcon />
						{{ route.params.id ? 'Create subcategory' : 'Create category' }}
					</Button>
				</div>
			</div>
		</template>

		<template #body>
			<div
				class="grid gap-4 lg:grid-cols-2 2xl:grid-cols-3"
				v-if="categories && categories.length > 0"
			>
				<div
					v-for="category in categories"
					:key="category.id"
					:class="`mt-2 ${
						category.deleted_at !== null ? 'opacity-50 hover:opacity-100' : ''
					}`"
					@dragleave="category.hoverClass = ''"
					@drop="drop($event, category)"
					@dragenter.prevent="category.hoverClass = 'bg-red-500'"
					@dragover.prevent="category.hoverClass = 'bg-red-500'"
				>
					<div
						class="cursor-pointer p-2 shadow transition-colors duration-300 hover:bg-gray-100 dark:bg-gray-900 hover:dark:bg-gray-800 md:p-5"
						:class="category.hoverClass"
						@click="
							$router.push({
								name: 'ProjectCategoryChildrenList',
								params: { id: category.id },
							})
						"
					>
						<div class="flex items-center justify-between">
							<div>
								<h3 class="text-xl font-bold">
									{{ category.title }}
								</h3>

								<div class="flex flex-wrap gap-3">
									<div
										class="flex items-center gap-1 font-semibold"
										title="subcategories"
									>
										<FolderClosedIcon class="size-4" />
										<span>{{ category.children_count }}</span>
									</div>

									<div
										class="flex items-center gap-1 font-semibold"
										title="tasks"
									>
										<ClipboardListIcon class="size-4" />
										<span>{{ category.tasks_count }}</span>
									</div>

									<span
										v-if="category.user?.name"
										class="flex items-center gap-1 font-semibold"
										title="author"
									>
										<CircleUserRoundIcon class="size-4" />
										{{ category.user.name }}
									</span>
								</div>
							</div>

							<div>
								<Button
									v-if="category.deleted_at === null"
									variant="destructive"
									@click.stop="deleteCategory(category)"
									title="delete category"
								>
									<span class="material-icons">delete</span>
								</Button>

								<Button
									v-else
									variant="default"
									@click.stop="restoreCategory(category)"
									title="restore"
								>
									<span class="material-icons">restore_from_trash</span>
								</Button>
							</div>
						</div>
					</div>
				</div>
			</div>

			<div class="mt-10">
				<div class="grid grid-cols-2 items-center sm:flex">
					<h2 class="text-white-800 text-left text-3xl lg:text-center">
						Tasks
					</h2>

					<Select
						v-model="workspaceStatus"
						:options="workspaceStatuses"
						class="ml-auto w-36"
						label-key="name"
						value-key="id"
					/>

					<div class="col-span-2 ml-auto flex items-center gap-2 sm:ml-2">
						<button class="opacity-25 hover:opacity-100" @click="selectAll">
							<span class="material-icons text-3xl">done_all</span>
						</button>

						<button
							class="opacity-25 hover:opacity-100"
							title="Add task to category"
							@click="
								$store.commit('createTaskInProjectCategoryId', {
									projectCategoryId: id,
								})
							"
						>
							<span class="material-icons text-3xl">add_circle_outline</span>
						</button>
					</div>
				</div>

				<tasks-list-component
					v-if="tasks && tasks.length > 0"
					ref="tasksListComponent"
					:is-loading-actions="isLoadingActions"
					:loading-action-tasks-ids="loadingActionTasksIds"
					:show-category-badges="false"
					:tasks="tasks"
					:use-task-status-for-buttons="true"
					draggable
					@reload-tasks="loadTasks"
				/>

				<div v-else class="mt-5 text-center text-xl italic">
					<EmptyState />
				</div>
			</div>
		</template>
	</BaseLayout>
</template>
