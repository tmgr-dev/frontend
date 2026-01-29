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
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
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
	import { Skeleton } from '@/components/ui/skeleton';
	import ForbiddenAccess from '@/components/ForbiddenAccess.vue';
	import FeatureGate from '@/components/general/FeatureGate.vue';
	import CategoriesPreview from '@/components/previews/CategoriesPreview.vue';
	import { FolderOpen } from 'lucide-vue-next';

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
	const workspaceCode = ref(null);
	const permissionDenied = ref(false);
	
	// Add categories pagination state
	const categoriesPagination = ref({
		current_page: parseInt(String(route.query.categories_page)) || 1,
		per_page: parseInt(String(route.query.categories_per_page)) || 10,
		total: 0,
		last_page: 1,
		from: 0,
		to: 0,
		path: '',
		links: {
			first: '',
			last: '',
			prev: null,
			next: null
		}
	});

	const pagination = ref({
		current_page: parseInt(String(route.query.page)) || 1,
		per_page: parseInt(String(route.query.per_page)) || 10,
		total: 0,
		last_page: 1,
		from: 0,
		to: 0
	});

	const perPageOptions = [
		{ label: '10 per page', value: 10 },
		{ label: '25 per page', value: 25 },
		{ label: '50 per page', value: 50 }
	];

	const categoriesPerPageOptions = [
		{ label: '10 per page', value: 10 },
		{ label: '25 per page', value: 25 },
		{ label: '50 per page', value: 50 }
	];

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

	watch(
		() => store.state.reloadTasksKey,
		() => {
			if (route.params.id) {
				loadTasks();
			}
		}
	);

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
			const response = await getTasks(
				{
					params: {
						project_category_id: route.params.id || null,
						status_id: route.params.status || null,
						page: pagination.value.current_page,
						per_page: pagination.value.per_page,
					},
				},
				false,
			);

			tasks.value = response.data;
			pagination.value = response.meta;
			setLoadingActions(tasks.value);
			isTasksFirstLoading.value = false;

			// Update URL with pagination parameters
			router.push({
				query: {
					...route.query,
					page: pagination.value.current_page,
					per_page: pagination.value.per_page
				}
			});
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
		try {
			const response = await getSubCategories(route.params.id || '', {
				params: {
					page: categoriesPagination.value.current_page,
					per_page: categoriesPagination.value.per_page
				},
			});
			
			// Update to handle the correct API response structure
			if (response?.data) {
				categories.value = response.data;
				if (response.meta) {
					categoriesPagination.value = {
						current_page: response.meta.current_page,
						per_page: response.meta.per_page,
						total: response.meta.total,
						last_page: response.meta.last_page,
						from: response.meta.from,
						to: response.meta.to,
						path: response.meta.path,
						links: response.links || {
							first: '',
							last: '',
							prev: null,
							next: null
						}
					};
				}
			} else {
				categories.value = [];
				console.error('Invalid response format:', response);
			}

			isCategoriesFirstLoading.value = false;

			// Update URL with categories pagination parameters
			router.push({
				query: {
					...route.query,
					categories_page: categoriesPagination.value.current_page,
					categories_per_page: categoriesPagination.value.per_page
				}
			});

			await loadParentCategory();
		} catch (error) {
			console.error('Error loading categories:', error);
			categories.value = [];
			isCategoriesFirstLoading.value = false;
		}
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

	const handlePageChange = (page) => {
		pagination.value.current_page = page;
		loadTasks();
	};

	const handlePerPageChange = (perPage) => {
		pagination.value.per_page = perPage;
		pagination.value.current_page = 1; // Reset to first page when changing items per page
		loadTasks();
	};

	const handleCategoriesPageChange = (page) => {
		categoriesPagination.value.current_page = page;
		loadCategories();
	};

	const handleCategoriesPerPageChange = (perPage) => {
		categoriesPagination.value.per_page = perPage;
		categoriesPagination.value.current_page = 1; // Reset to first page when changing items per page
		loadCategories();
	};

	const getWorkspaceCode = () => {
		const currentWorkspaceId = store.state.user?.settings?.find(
			setting => setting.key === 'current_workspace'
		)?.value;
		
		const currentWorkspace = store.state.workspaces?.find(
			workspace => Number(workspace.id) === Number(currentWorkspaceId)
		);
		console.log('Current workspace code:', currentWorkspace?.code);
		return currentWorkspace?.code;
	};

	watch(category, (newCategory) => {
		const title = newCategory ? newCategory.title : 'Categories';
		setDocumentTitle(title);
	}, { immediate: true });

	onMounted(async () => {
		try {
			workspaceCode.value = route.params.workspace_code || store.state.user?.workspace_code;
			workspaceStatus.value = route.params.status || 'all';

			// Initialize pagination from URL query parameters
			if (route.query.categories_page) {
				categoriesPagination.value.current_page = parseInt(String(route.query.categories_page));
			}
			if (route.query.categories_per_page) {
				categoriesPagination.value.per_page = parseInt(String(route.query.categories_per_page));
			}
			if (route.query.page) {
				pagination.value.current_page = parseInt(String(route.query.page));
			}
			if (route.query.per_page) {
				pagination.value.per_page = parseInt(String(route.query.per_page));
			}
			await Promise.all([
				loadCategories(),
				loadTasks(),
			]);
		} catch (error) {
			console.error('Error loading category page data:', error);
			if (error.response?.status === 403) {
				permissionDenied.value = true;
				isCategoriesFirstLoading.value = false;
				isTasksFirstLoading.value = false;
			} else {
				// Handle other errors (e.g., show a generic error message)
			}
		}
	});
</script>

<template>
	<div>
		<ForbiddenAccess v-if="permissionDenied" />

		<FeatureGate
		v-else
		feature-key="categories"
		title="Project Categories"
		description="Organize your tasks into categories and subcategories. Create a hierarchical structure for better project management."
		:icon="FolderOpen"
	>
		<template #preview>
			<CategoriesPreview />
		</template>

	<BaseLayout>
		<template #action>
			<div
				class="flex flex-col justify-between gap-2 sm:flex-row sm:items-center sm:gap-4"
			>
				<div class="flex items-center gap-2">
					<breadcrumbs
						:current="category ? category.title : ''"
						:drop="drop"
						:items="getBreadcrumbs(parentCategories)"
					/>
				</div>

				<div class="flex gap-3">
					<Button
						v-if="category"
						:title="`Edit ${category.title} category`"
						variant="outline"
						@click="() => {
							if (workspaceCode) {
								router.push(`/${workspaceCode}/categories/${category.id}`);
							} else {
								router.push(`/projects-categories/${category.id}`);
							}
						}"
					>
						<FolderPenIcon />
						{{ category.title }}
					</Button>

					<Button
						@click="() => {
							if (workspaceCode) {
								router.push(
									`/${workspaceCode}/categories/${
										route.params.id ? `${route.params.id}/` : ''
									}create`
								);
							} else {
								router.push(
									`/projects-categories/${
										route.params.id ? `${route.params.id}/` : ''
									}create`
								);
							}
						}"
					>
						<FolderPlusIcon />
						{{ route.params.id ? 'Create subcategory' : 'Create category' }}
					</Button>
				</div>
			</div>
		</template>

		<template #body>
			<div
				v-if="isCategoriesFirstLoading"
				class="mt-8 grid gap-4 lg:grid-cols-2 2xl:grid-cols-3 min-h-48"
			>
				<Skeleton class="h-24 w-full" />
				<Skeleton class="hidden h-24 w-full lg:block" />
				<Skeleton class="hidden h-24 w-full 2xl:block" />
			</div>

			<div v-else>
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
								if (workspaceCode) {
									$router.push({
										name: 'WorkspaceCategoryChildren',
										params: { workspace_code: workspaceCode, id: category.id }
									});
								} else {
									$router.push({
										name: 'ProjectCategoryChildrenList',
										params: { id: category.id }
									});
								}
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
												() => {
													if (workspaceCode) {
														router.push(`/${workspaceCode}/categories/${category.id}`);
													} else {
														router.push(`/projects-categories/${category.id}`);
													}
												}
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

				<!-- Add categories pagination controls -->
				<div v-if="categories && categories.length > 0" class="mt-4 flex items-center justify-between px-4">
					<div class="flex items-center gap-2">
						<span class="text-sm text-gray-600 dark:text-gray-300">
							Showing {{ categoriesPagination.from }} to {{ categoriesPagination.to }} of {{ categoriesPagination.total }} categories
						</span>
						
						<Select
							v-model="categoriesPagination.per_page"
							:options="categoriesPerPageOptions"
							class="w-36"
							label-key="label"
							value-key="value"
							@update:modelValue="handleCategoriesPerPageChange"
						/>
					</div>
					
					<div v-if="categoriesPagination.total > categoriesPagination.per_page" class="flex items-center gap-2">
						<Button
							:disabled="categoriesPagination.current_page === 1"
							@click="handleCategoriesPageChange(categoriesPagination.current_page - 1)"
							variant="outline"
							class="px-3 py-1"
						>
							Previous
						</Button>
						
						<span class="text-sm text-gray-600 dark:text-gray-300">
							Page {{ categoriesPagination.current_page }} of {{ categoriesPagination.last_page }}
						</span>
						
						<Button
							:disabled="categoriesPagination.current_page === categoriesPagination.last_page"
							@click="handleCategoriesPageChange(categoriesPagination.current_page + 1)"
							variant="outline"
							class="px-3 py-1"
						>
							Next
						</Button>
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

					<Button
						title="Create a task in this category"
						@click="
							$store.commit('createTaskInProjectCategoryId', {
								projectCategoryId: Number(route.params.id),
								statusId: null,
							})
						"
					>
						<span class="material-icons text-lg mr-1">add_circle_outline</span>
						Create task
					</Button>
					</div>
				</div>

				<div v-if="isTasksFirstLoading" class="mt-6 space-y-2 px-2 min-h-96">
					<Skeleton class="h-28 w-full" />
					<Skeleton class="h-28 w-full" />
					<Skeleton class="h-28 w-full" />
				</div>

				<tasks-list-component
					v-if="tasks && tasks.length > 0"
					:tasks="tasks"
					:is-loading-actions="isLoadingActions"
					:loading-action-tasks-ids="loadingActionTasksIds"
					:pagination="pagination"
					@reload-tasks="loadTasks"
					@page-change="handlePageChange"
					@per-page-change="handlePerPageChange"
					ref="tasksListComponent"
					draggable
				/>

				<div v-else-if="!isTasksFirstLoading" class="mt-5 text-center text-xl italic">
					<EmptyState />
				</div>
			</div>
		</template>
	</BaseLayout>

		</FeatureGate>
	</div>
</template>
