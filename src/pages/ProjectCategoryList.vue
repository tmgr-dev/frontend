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
		EllipsisIcon,
		Trash2Icon,
		ArchiveRestoreIcon,
	} from 'lucide-vue-next';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';

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

	watch(workspaceStatus, (newStatus, prevStatus) => {
		if (newStatus === prevStatus) {
			return;
		}

		if (route.params.id) {
			router.push(
				`/projects-categories/${
					route.params.id ? `${route.params.id}/children` : '/status'
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
		if (route.params.id) {
			tasks.value = await getTasks(
				{
					params: {
						project_category_id: route.params.id,
						status_id: route.params.status,
					},
				},
				false,
			);

			setLoadingActions(tasks.value);
			isTasksFirstLoading.value = false;
		}
	};

	const loadParentCategory = async () => {
		if (route.params.id) {
			category.value = await getParentCategory(route.params.id);
			store.commit('setMetaTitle', category.value.title || 'Categories');
			parentCategories.value = extractParents(category.value);
		}
	};

	const loadCategories = async () => {
		categories.value = await getSubCategories(route.params.id || '', {
			params: {
				all: '',
			},
		});

		await loadParentCategory();
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

		if (route.params.status) {
			workspaceStatus.value =
				route.params.status === 'all'
					? route.params.status
					: parseInt(route.params.status);
		}
	});
</script>

<template>
	<teleport to="title">
		{{ category ? category.title : 'Categories' }}
	</teleport>

	<BaseLayout>
		<template #action>
			<div
				class="flex flex-col justify-between gap-2 sm:flex-row sm:items-center sm:gap-4"
			>
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
							() =>
								router.push(
									`/projects-categories/${
										route.params.id && `${route.params.id}/`
									}create`,
								)
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
				class="mt-6 grid gap-4 lg:grid-cols-2 2xl:grid-cols-3"
				v-if="categories && categories.length > 0"
			>
				<div
					v-for="category in categories"
					:key="category.id"
					class="mt-2 h-full"
					:class="[
						category.deleted_at !== null && 'opacity-40 hover:opacity-50',
					]"
					@dragleave="category.hoverClass = ''"
					@drop="drop($event, category)"
					@dragenter.prevent="category.hoverClass = 'bg-red-500'"
					@dragover.prevent="category.hoverClass = 'bg-red-500'"
				>
					<div
						class="relative h-full cursor-pointer p-2 !pr-12 shadow transition hover:bg-gray-100 dark:bg-gray-900 hover:dark:bg-gray-800 md:p-5"
						:class="category.hoverClass"
						@click="
							$router.push({
								name: 'ProjectCategoryChildrenList',
								params: { id: category.id },
							})
						"
					>
						<h3 class="text-xl font-bold">
							{{ category.title }}
						</h3>

						<p v-if="category.deleted_at !== null">
							(deleted, but can be restored)
						</p>

						<div class="flex flex-wrap gap-3">
							<div
								class="flex items-center gap-1 font-semibold"
								title="subcategories"
							>
								<FolderClosedIcon class="size-4" />
								<span>{{ category.children_count }}</span>
							</div>

							<div class="flex items-center gap-1 font-semibold" title="tasks">
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

						<div class="absolute right-4 top-1/2 z-50 -translate-y-1/2">
							<DropdownMenu>
								<DropdownMenuTrigger class="hover:opacity-70" @click.stop>
									<EllipsisIcon />
								</DropdownMenuTrigger>

								<DropdownMenuContent class="mr-4 mt-1">
									<DropdownMenuItem
										@click="
											() => router.push(`/projects-categories/${category.id}`)
										"
									>
										<FolderPenIcon />
										<span>Edit</span>
									</DropdownMenuItem>

									<DropdownMenuItem
										v-if="category.deleted_at === null"
										@click="deleteCategory(category)"
									>
										<Trash2Icon />
										<span>Delete</span>
									</DropdownMenuItem>

									<DropdownMenuItem v-else @click="restoreCategory(category)">
										<ArchiveRestoreIcon />
										<span>Restore</span>
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</div>
					</div>
				</div>
			</div>

			<div v-if="route.params.id" class="mt-10">
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
