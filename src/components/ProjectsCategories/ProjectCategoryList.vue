<template>
	<teleport to="title">
		{{ category ? category.title : h1 }}
	</teleport>

	<BaseLayout>
		<template #action>
			<breadcrumbs
				:current="category ? category.title : ''"
				:drop="drop"
				:items="getBreadcrumbs(parentCategories)"
			/>

			<div class="md:absolute right-0 bottom-0 mr-5 mb-2">
				<router-link
					v-if="category"
					:to="`/projects-categories/${category.id}/edit`"
					class="pr-5 opacity-25 hover:opacity-100"
					title="Edit category name"
				>
					<span class="material-icons text-4xl">edit</span>
				</router-link>

				<router-link
					:to="`/projects-categories/${id ? id + '/' : ''}create`"
					class="opacity-25 hover:opacity-100"
					title="Add category"
				>
					<span class="material-icons text-4xl">add_circle_outline</span>
				</router-link>
			</div>
		</template>

		<template #body>
			<loading-tasks-list v-if="isCategoriesFirstLoading" />

			<div v-if="categories && categories.length > 0">
				<div
					v-for="category in categories"
					:key="category.id"
					:class="`w-full mt-2 ${
						category.deleted_at !== null ? 'hover:opacity-100 opacity-50' : ''
					}`"
					@dragleave="category.hoverClass = ''"
					@drop="drop($event, category)"
					@dragenter.prevent="category.hoverClass = 'bg-red-500'"
					@dragover.prevent="category.hoverClass = 'bg-red-500'"
				>
					<div class="shadow-md rounded-lg md:flex">
						<div class="w-full">
							<div
								class="dark:bg-gray-900 transition-colors duration-300 bg-white hover:bg-gray-100 hover:dark:bg-gray-800 p-4 md:p-5"
								:class="category.hoverClass"
							>
								<div class="flex justify-between items-center">
									<div>
										<p
											class="font-bold text-xl cursor-pointer text-left pl-2"
											@click="
												$router.push({
													name: 'ProjectCategoryChildrenList',
													params: { id: category.id },
												})
											"
										>
											{{ category.title }}
										</p>
										<div class="flex items-start">
											<span class="text-gray-700 ml-2"
												>Projects: {{ category.children_count }}; Tasks:
												{{ category.tasks_count }}</span
											>
										</div>
									</div>

									<div v-if="category.deleted_at === null">
										<DropdownMenu
											:actions="getActions(category)"
											class="lg:hidden"
										></DropdownMenu>
										<div class="hidden lg:block">
											<new-button
												class="mr-2"
												@click="
													$router.push({
														name: 'ProjectCategoryChildrenList',
														params: { id: category.id },
													})
												"
											>
												<span class="material-icons">open_in_new</span>
											</new-button>
											<new-button
												class="mr-2"
												color="red"
												@click="deleteCategory(category)"
											>
												<span class="material-icons">delete</span>
											</new-button>
										</div>
									</div>
									<div v-else>
										<new-button class="mr-2" @click="restoreCategory(category)">
											<span class="material-icons">restore_from_trash</span>
										</new-button>
									</div>
								</div>
							</div>
						</div>
					</div>
				</div>
			</div>
			<div
				v-else-if="!isCategoriesFirstLoading"
				class="text-center italic text-xl"
			>
				You don't have categories here
			</div>

			<div class="mt-5">
				<h1
					class="text-3xl text-white-800 pt-5 relative text-left lg:text-center ml-2"
				>
					<span>
						Tasks
						<a
							class="opacity-25 hover:opacity-100 inline md:hidden"
							href="#"
							@click.prevent="selectAll"
						>
							<span class="material-icons text-3xl">done_all</span>
						</a>
						<a
							:href="`/${
								id ? 'project-categories/' + id + '/tasks/' : ''
							}create`"
							class="opacity-25 hover:opacity-100 inline md:hidden"
							title="Add task to category"
							@click.prevent="
								$store.commit('createTaskInProjectCategoryId', {
									projectCategoryId: id,
								})
							"
						>
							<span class="material-icons text-3xl">add_circle_outline</span>
						</a>
					</span>
					<span
						class="md:absolute md:mt-2 md:mt-0 flex right-0 bottom-0 sm:mr-5"
					>
						<span class="sm:mr-5 text-lg">
							<input-field
								v-if="workspaceStatuses.length > 0"
								v-model="workspaceStatus"
								:options="workspaceStatuses"
								class="items-center"
								option-name-key="name"
								option-value-key="id"
								style="min-width: 200px"
								type="select"
							/>
						</span>
						<a
							class="opacity-25 hover:opacity-100 hidden md:inline mr-2"
							href="#"
							title="Select all"
							@click.prevent="selectAll"
						>
							<span class="material-icons text-3xl">done_all</span>
						</a>
						<a
							:href="`/${
								id ? 'project-categories/' + id + '/tasks/' : ''
							}create`"
							class="opacity-25 hover:opacity-100 hidden md:inline"
							title="Add task to category"
							@click.prevent="
								$store.commit('createTaskInProjectCategoryId', {
									projectCategoryId: id,
								})
							"
						>
							<span class="material-icons text-4xl">add_circle_outline</span>
						</a>
					</span>
				</h1>

				<loading-tasks-list v-if="isTasksFirstLoading" class="mt-2" />

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
					You don't have tasks in the category
				</div>
			</div>
		</template>
	</BaseLayout>

	<Transition name="fade">
		<confirm
			v-if="confirm"
			:body="confirm.body"
			:title="confirm.title"
			@onCancel="confirm = undefined"
			@onOk="confirm.action()"
		/>
	</Transition>
</template>

<script>
	import Confirm from '../UIElements/Confirm';
	import Breadcrumbs from '../UIElements/Breadcrumbs';
	import extractParents from '../../utils/extractParents';
	import LoadingButtonActions from 'src/mixins/LoadingButtonActions';
	import getBreadcrumbs from '../../utils/breadcrumbs/getBreadcrumbs';
	import LoadingTasksList from 'src/components/Loaders/LoadingTasksList.vue';
	import TasksListComponent from 'src/components/UIElements/Tasks/TasksListComponent';
	import InputField from 'src/components/UIElements/InputField';
	import { getTasks, updateTaskPartially } from 'src/actions/tmgr/tasks';
	import {
		restoreCategory,
		deleteCategory as deleteCategoryAction,
		getParentCategory,
		getSubCategories,
	} from 'src/actions/tmgr/categories';

	export default {
		name: 'ProjectCategoryList',
		components: {
			InputField,
			LoadingTasksList,
			Confirm,
			Breadcrumbs,
			TasksListComponent,
		},
		mixins: [LoadingButtonActions],
		data: () => ({
			h1: 'Projects categories',
			isShowModal: false,
			cardOpen: true,
			item: true,
			tasks: null,
			categories: null,
			category: null,
			parentCategories: [],
			isCategoriesFirstLoading: true,
			isTasksFirstLoading: true,
			isLoadingActions: {},
			confirm: null,
			loadingActionTasksIds: [],
			workspaceStatus: 'all',
		}),
		computed: {
			workspaceStatuses() {
				let statuses = this.$store.getters.statuses;

				return [
					{
						name: 'All',
						id: 'all',
					},
				].concat(statuses);
			},
			id() {
				return this.$route.params.id || '';
			},
			status() {
				return this.$route.params.status || '';
			},
		},
		watch: {
			workspaceStatus(newVal, oldVal) {
				if (newVal === oldVal) {
					return;
				}
				const id = this.id;
				if (id) {
					this.$router.push(
						`/projects-categories/${
							id ? `${id}/children` : '/status'
						}/${newVal}`,
					);
				} else {
					this.$router.push({
						name: 'ProjectCategoryChildrenListWithStatus',
						params: { status: newVal },
					});
				}
			},
		},
		methods: {
			async drop(event, category) {
				const taskId = +event.dataTransfer.getData('task-id');
				const categoryId = category.id;

				category.hoverClass = '';
				this.loadingActionTasksIds.push(taskId);

				await updateTaskPartially(taskId, {
					project_category_id: categoryId,
				});

				await this.loadCategories();
				await this.loadTasks();
				this.loadingActionTasksIds = this.loadingActionTasksIds.filter(
					(id) => id !== taskId,
				);
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async loadTasks() {
				this.tasks = await getTasks({
					params: {
						project_category_id: this.id,
						status_id: this.status || null,
					},
				});

				this.setLoadingActions(this.tasks);
				this.isTasksFirstLoading = false;
			},
			getBreadcrumbs,
			getActions(category) {
				return [
					{
						click: () => {
							return this.$router.push({
								name: 'ProjectCategoryChildrenList',
								params: { id: category.id },
							});
						},
						label: 'Open',
					},
					{
						click: () => {
							this.deleteCategory(category);
						},
						label: 'Delete',
					},
				];
			},
			async loadParentCategory() {
				this.category = await getParentCategory(this.id);
				this.parentCategories = this.extractParents(this.category);
			},
			async loadCategories() {
				this.categories = await getSubCategories(this.id, {
					params: {
						all: '',
					},
				});

				if (this.id) {
					await this.loadParentCategory();
				}

				this.isCategoriesFirstLoading = false;
			},
			async deleteCategory(category) {
				this.showConfirm('Delete category', 'Are you sure?', async () => {
					try {
						category.deleted_at = await deleteCategoryAction(category.id);
						await this.loadTasks();
					} catch (e) {
						console.error(e);
					} finally {
						this.confirm = null;
					}
				});
			},
			async restoreCategory(category) {
				try {
					category.deleted_at = await restoreCategory(category.id);
					await this.loadTasks();
				} catch (e) {
					console.error(e);
				}
			},
			extractParents,
			selectAll() {
				if (!this.$refs.tasksListComponent) {
					return;
				}
				this.$refs.tasksListComponent.selectAll();
			},
		},
		async mounted() {
			await this.loadCategories();
			await this.loadTasks();
			const status = this.$route.params.status;
			if (!status) {
				return;
			}
			this.workspaceStatus = status === 'all' ? status : parseInt(status);
		},
	};
</script>

<style lang="scss" scoped>
	.my-adding {
		@media (max-width: 500px) {
			position: static;
			display: block;
			margin-left: auto;
			margin-right: 0;
		}
	}

	.actions-wrapper {
		.delete-button {
			margin-left: 90px;
		}
	}
</style>
