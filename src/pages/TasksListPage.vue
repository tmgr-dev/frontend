<script setup>
	import LoadingTasksList from '@/components/loaders/LoadingTasksList.vue';
	import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
	import Confetti from '@/components/Confetti.vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import { getTasks, getTasksByStatus } from '@/actions/tmgr/tasks';
	import TextField from '@/components/general/TextField.vue';
	import { getCategories } from '@/actions/tmgr/categories';
	import Select from '@/components/general/Select.vue';
	import { computed, onMounted, ref } from 'vue';
	import { useRoute } from 'vue-router';

	const route = useRoute();
	const showCreateTaskForm = ref(false);
	const selectableTasks = ref(false);
	const errorLoading = ref(false);
	const showSearchInput = ref(false);
	const panel = ref(false);
	const searchText = ref(null);
	const searchTimeout = ref(null);
	const summaryTimeString = ref(null);
	const isLoading = ref(true);
	const h1 = {
		CurrentTasksList: 'Current tasks',
		HiddenTasksList: 'Hidden tasks',
		ArchiveTasksList: 'Archive tasks',
	};
	const tasks = ref([]);
	const isLoadingActions = ref({});
	const hasAbilityToShowConfetti = ref(false);
	const categories = ref([]);
	const selectedCategory = ref(null);
	const showCategorySelect = ref(false);
	const tasksListComponentRef = ref(null);

	const status = computed(() => route.meta.status);

	onMounted(async () => {
		try {
			const categoriesData = await getCategories();

			categories.value = [
				{ id: 0, title: 'All categories' },
				...categoriesData.map((category) => ({
					id: category.id,
					title: category.title,
				})),
			];
			const data = await loadTasks();
			setLoadingActions(data);
		} catch (e) {
			console.error(e);
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

	function findByCategory(id) {
		if (!id) {
			return;
		} else {
			const filteredTasks = tasks.value.filter(
				(task) => task.category && task.category.id === id,
			);
			tasks.value = filteredTasks;
		}
	}

	async function reloadTasks() {
		hasAbilityToShowConfetti.value = true;
		await loadTasks();
	}

	async function loadTasks() {
		try {
			isLoading.value = true;
			clearTimeout(searchTimeout.value);

			let fetchedTasks = [];

			if (status.value) {
				fetchedTasks = await getTasksByStatus(status.value, {
					params: {
						search: searchText.value,
					},
				});
			} else {
				fetchedTasks = await getTasks({
					params: {
						search: searchText.value,
					},
				});
			}

			summaryTimeString.value = formatTime(
				fetchedTasks.reduce((summary, task) => task.common_time + summary, 0),
			);
			tasks.value = fetchedTasks;

			return fetchedTasks;
		} catch (e) {
			console.error(e);
			errorLoading.value = true;
		} finally {
			isLoading.value = false;
		}
	}

	function formatTime(taskTime) {
		let hours = Math.floor(taskTime / 3600);
		let minutes = Math.ceil((taskTime % 3600) / 60);

		return `${
			hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''
		} ${minutes} minute${minutes > 1 ? 's' : ''}`;
	}
</script>

<template>
	<teleport to="title">
		{{ h1[route.name] }}
	</teleport>

	<BaseLayout>
		<template #action>
			<div class="items-center justify-between px-2 md:flex md:flex-nowrap">
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

					<button type="button" @click="selectableTasks = !selectableTasks">
						<svg
							viewBox="0 0 1024 1024"
							class="size-10"
							:class="[selectableTasks ? 'fill-gray-400' : 'fill-gray-700']"
							xmlns="http://www.w3.org/2000/svg"
						>
							<path
								d="M598.186667 955.733333h-0.443734a17.117867 17.117867 0 0 1-15.240533-10.325333L476.859733 699.733333H426.666667a17.066667 17.066667 0 1 1 0-34.133333h35.5328l-136.533334-317.525333a17.066667 17.066667 0 0 1 22.408534-22.408534l317.525333 136.533334V426.666667a17.066667 17.066667 0 1 1 34.133333 0v50.158933l245.640534 104.8064a17.083733 17.083733 0 0 1 0.9216 30.976l-149.486934 74.752 153.924267 153.924267a17.0496 17.0496 0 0 1 0 24.132266l-85.333333 85.333334a17.0496 17.0496 0 0 1-24.132267 0l-153.856-153.856-73.9328 149.3504a17.066667 17.066667 0 0 1-15.291733 9.489066zM682.666667 750.933333a17.066667 17.066667 0 0 1 12.066133 5.000534L853.333333 914.5344 914.5344 853.333333l-158.600533-158.600533a17.015467 17.015467 0 0 1 4.437333-27.323733l137.6768-68.829867-222.037333-94.737067-0.1024-0.034133-301.9776-129.877333 129.860266 301.994666 0.034134 0.068267 95.453866 222.020267 68.096-137.5744A17.066667 17.066667 0 0 1 682.666667 750.933333z m-341.333334-51.2h-85.333333a17.066667 17.066667 0 1 1 0-34.133333h85.333333a17.066667 17.066667 0 1 1 0 34.133333z m-170.666666 0H85.333333a17.066667 17.066667 0 0 1-17.066666-17.066666v-85.333334a17.066667 17.066667 0 1 1 34.133333 0v68.266667h68.266667a17.066667 17.066667 0 1 1 0 34.133333zM85.333333 529.066667a17.066667 17.066667 0 0 1-17.066666-17.066667v-85.333333a17.066667 17.066667 0 0 1 34.133333 0v85.333333a17.066667 17.066667 0 0 1-17.066667 17.066667z m597.333334-170.666667a17.066667 17.066667 0 0 1-17.066667-17.066667v-85.333333a17.066667 17.066667 0 1 1 34.133333 0v85.333333a17.066667 17.066667 0 0 1-17.066666 17.066667zM85.333333 358.4a17.066667 17.066667 0 0 1-17.066666-17.066667v-85.333333a17.066667 17.066667 0 0 1 34.133333 0v85.333333a17.066667 17.066667 0 0 1-17.066667 17.066667z m597.333334-170.666667a17.066667 17.066667 0 0 1-17.066667-17.066666V102.4h-68.266667a17.066667 17.066667 0 1 1 0-34.133333h85.333334a17.066667 17.066667 0 0 1 17.066666 17.066666v85.333334a17.066667 17.066667 0 0 1-17.066666 17.066666zM85.333333 187.733333a17.066667 17.066667 0 0 1-17.066666-17.066666V85.333333a17.066667 17.066667 0 0 1 17.066666-17.066666h85.333334a17.066667 17.066667 0 0 1 0 34.133333H102.4v68.266667a17.066667 17.066667 0 0 1-17.066667 17.066666z m426.666667-85.333333h-85.333333a17.066667 17.066667 0 0 1 0-34.133333h85.333333a17.066667 17.066667 0 1 1 0 34.133333zM341.333333 102.4h-85.333333a17.066667 17.066667 0 0 1 0-34.133333h85.333333a17.066667 17.066667 0 0 1 0 34.133333z"
							/>
						</svg>
					</button>

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
				:has-selectable="selectableTasks"
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
		</template>
	</BaseLayout>
</template>

<!--<script>
	import LoadingTasksList from '@/components/loaders/LoadingTasksList.vue';
	import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
	import Confetti from '@/components/Confetti.vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import { getTasks, getTasksByStatus } from '@/actions/tmgr/tasks';
	import TextField from '@/components/general/TextField.vue';
	import { getCategories } from '@/actions/tmgr/categories';
	import Select from '@/components/general/Select.vue';

	export default {
		name: 'TasksList',
		components: {
			TextField,
			TaskForm,
			Confetti,
			LoadingTasksList,
			TasksListComponent,
			Select,
		},
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

					this.summaryTimeString = this.formatTime(
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
			formatTime(taskTime) {
				let hours = Math.floor(taskTime / 3600);
				let minutes = Math.ceil((taskTime % 3600) / 60);

				return `${
					hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''
				}  ${minutes} minute${minutes > 1 ? 's' : ''}`;
			},
		},
		async created() {
			const data = await this.loadTasks();
			this.setLoadingActions(data);
		},
	};
</script>-->
