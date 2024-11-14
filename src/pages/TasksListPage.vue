<template>
	<teleport to="title">
		{{ h1[$route.name] }}
	</teleport>
	<teleport to="#breadcrumb">
		<BreadcrumbItem class="hidden md:block">
			<BreadcrumbLink href="#">
				{{ h1[$route.name] }}
			</BreadcrumbLink>
		</BreadcrumbItem>
	</teleport>

	<BaseLayout>
		<template #action>
			<div class="items-center justify-between md:flex md:flex-nowrap">
				<transition name="fade">
					<div
						v-if="summaryTimeString"
						class="text-bold my-5 mr-6 shrink-0 text-center text-lg text-opacity-25 sm:w-auto sm:text-xl lg:text-2xl"
					>
						{{ summaryTimeString }}
					</div>
				</transition>

				<div
					class="absolute top-14 flex w-[90%] items-center justify-center p-4 md:static"
				>
					<div
						class="mr-3 w-1/2 overflow-hidden md:ml-auto md:w-full lg:w-1/3 xl:w-1/5"
					>
						<transition name="transform-opacity-right" mode="out-in">
							<TextField
								v-if="showSearchInput"
								placeholder="Enter task name"
								v-model="searchText"
								class="p-2 md:p-0"
							/>
						</transition>
					</div>

					<div
						v-if="categories.length >= 2"
						class="w-44 md:m-0 md:ml-3 md:mr-3"
						:class="{ hidden: !showCategorySelect }"
					>
						<transition name="transform-opacity-right" mode="out-in">
							<Select
								placeholder="Select category"
								:options="categories"
								v-model="selectedCategory"
								label-key="title"
								value-key="id"
								class="p-2 md:p-0"
							/>
						</transition>
					</div>
				</div>

				<div class="ml-0 mt-14 w-full text-center sm:w-auto md:mt-0 md:flex">
					<a
						href="#"
						@click.prevent="showCategorySelect = !showCategorySelect"
						title="Search tasks"
						class="pr-1"
					>
						<span
							class="material-icons text-3xl text-gray-700 opacity-75 hover:opacity-100 sm:text-4xl"
						>
							{{ showCategorySelect ? 'filter_list' : 'filter_alt' }}
						</span>
					</a>
					<a
						href="#"
						@click.prevent="showSearchInput = !showSearchInput"
						title="Search tasks"
						class="pr-1"
					>
						<span
							class="material-icons text-3xl text-gray-700 opacity-75 hover:opacity-100 sm:text-4xl"
						>
							{{ showSearchInput ? 'search_off' : 'search' }}
						</span>
					</a>

					<a
						href="#"
						@click.prevent="selectAll()"
						title="Select all"
						class="pr-1"
					>
						<span
							class="material-icons text-3xl text-gray-700 opacity-75 hover:opacity-100 sm:text-4xl"
						>
							done_all
						</span>
					</a>

					<a
						href="#"
						title="Add Task"
						@click="$store.commit('setShowCreatingTaskModal')"
					>
						<span
							class="material-icons text-3xl text-gray-700 opacity-75 hover:opacity-100 sm:text-4xl"
						>
							add_circle_outline
						</span>
					</a>
				</div>
			</div>
		</template>

		<template #body>
			<tasks-list-component
				v-if="tasks && tasks.length > 0 && !isLoading"
				:tasks="tasks"
				:status="status"
				:is-loading-actions="isLoadingActions"
				has-selectable
				@reload-tasks="reloadTasks"
				ref="tasksListComponent"
			/>

			<div v-else-if="errorLoading" class="text-center text-xl italic">
				Something went wrong...
			</div>

			<div v-else-if="!isLoading" class="text-center text-xl italic">
				You don't have tasks here

				<confetti v-if="hasAbilityToShowConfetti" />
			</div>

			<loading-tasks-list v-if="isLoading" class="mx-2" />
			<!--<loader v-if="showLoader" style="margin-top: 2rem" />-->
		</template>
	</BaseLayout>
</template>

<script>
	import TasksListMixin from '@/mixins/TasksListMixin';
	import LoadingButtonActions from '@/mixins/LoadingButtonActions';
	import LoadingTasksList from '@/components/loaders/LoadingTasksList.vue';
	import TasksListComponent from '@/components/tasks/TasksList.vue';
	import Confetti from '@/components/Confetti.vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import { getTasks, getTasksByStatus } from '@/actions/tmgr/tasks';
	import TextField from '@/components/general/TextField.vue';
	import { getCategories } from '@/actions/tmgr/categories';
	import { hueFromHex } from '@/utils/convertColors';
	import { computed } from 'vue';
	import Select from '@/components/general/Select.vue';
	import { BreadcrumbItem, BreadcrumbLink } from '@/components/ui/breadcrumb';

	export default {
		name: 'TasksList',
		components: {
			BreadcrumbItem, BreadcrumbLink,
			TextField,
			TaskForm,
			Confetti,
			LoadingTasksList,
			TasksListComponent,
			Select,
		},
		mixins: [LoadingButtonActions, TasksListMixin],
		data: () => ({
			showCreateTaskForm: false,
			errorLoading: false,
			showSearchInput: false,
			panel: false,
			searchText: null,
			searchTimeout: null,
			summaryTimeString: null,
			isLoading: true,
			h1: {
				CurrentTasksList: 'Current tasks',
				HiddenTasksList: 'Hidden tasks',
				ArchiveTasksList: 'Archive tasks',
			},
			tasks: [],
			isLoadingActions: {},
			hasAbilityToShowConfetti: false,
			categories: [],
			selectedCategory: null,
			showCategorySelect: false,
		}),
		computed: {
			status() {
				return this.$route.meta.status;
			},
		},
		async mounted() {
			const categoriesData = await getCategories();
			this.categories = [
				{ id: 0, title: 'All categories' },
				...categoriesData.map((cat) => ({
					id: cat.id,
					title: cat.title,
				})),
			];
		},

		watch: {
			searchText() {
				clearTimeout(this.searchTimeout);
				this.searchTimeout = setTimeout(this.loadTasks, 500);
			},
			'$store.getters.reloadTasks'() {
				this.loadTasks();
			},
			async selectedCategory(newCategory, oldCategory) {
				await this.loadTasks();
				this.selectedCategory = newCategory;
				this.findByCategory(this.selectedCategory);
			},
		},
		methods: {
			findByCategory(id) {
				if (!id) {
					return;
				} else {
					const tasks = this.tasks.filter(
						(task) => task.category && task.category.id === id,
					);
					this.tasks = tasks;
				}
			},

			reloadTasks() {
				this.hasAbilityToShowConfetti = true;
				this.loadTasks();
			},

			async loadTasks() {
				try {
					this.isLoading = true;
					clearTimeout(this.searchTimeout);

					let tasks = [];

					if (this.status) {
						tasks = await getTasksByStatus(this.status, {
							params: {
								search: this.searchText,
							},
						});
					} else {
						tasks = await getTasks({
							params: {
								search: this.searchText,
							},
						});
					}

					this.summaryTimeString = this.getTaskFormattedTime(
						tasks.reduce((summary, task) => task.common_time + summary, 0),
					);
					this.tasks = tasks;

					return tasks;
				} catch (e) {
					console.error(e);
					this.errorLoading = true;
				} finally {
					this.isLoading = false;
				}
			},
			selectAll() {
				if (!this.$refs.tasksListComponent) {
					return;
				}
				this.$refs.tasksListComponent.selectAll();
			},
		},
		async created() {
			const data = await this.loadTasks();
			this.setLoadingActions(data);
		},
	};
</script>
