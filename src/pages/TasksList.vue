<template>
	<teleport to="title">
		{{ h1[$route.name] }}
	</teleport>

	<BaseLayout>
		<template #action>
			<div class="flex flex-nowrap items-center justify-between">
				<transition name="fade">
					<div
						v-if="summaryTimeString"
						class="ext-opacity-25 text-bold mr-6 w-full shrink-0 text-center text-lg sm:w-auto sm:text-xl lg:text-2xl"
					>
						{{ summaryTimeString }}
					</div>
				</transition>

				<div
					class="ml-auto mr-3 w-full overflow-hidden md:w-1/2 lg:w-1/4 xl:w-1/5"
				>
					<transition name="transform-opacity-right" mode="out-in">
						<TextField
							v-if="showSearchInput"
							placeholder="Enter task name"
							v-model="searchText"
						/>
					</transition>
				</div>

				<div
					class="ml-0 mt-2 hidden w-full shrink-0 text-center sm:mt-0 sm:w-auto md:flex"
				>
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
						@click="$store.dispatch('showCreateTaskModal')"
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
	import TasksListMixin from 'src/mixins/TasksListMixin';
	import LoadingButtonActions from 'src/mixins/LoadingButtonActions';
	import LoadingTasksList from 'src/components/loaders/LoadingTasksList.vue';
	import TasksListComponent from 'src/components/tasks/TasksListComponent.vue';
	import Confetti from 'src/components/UIElements/Confetti.vue';
	import TaskForm from 'src/pages/TaskForm.vue';
	import { getTasks, getTasksByStatus } from 'src/actions/tmgr/tasks';
	import TextField from 'src/components/general/TextField.vue';

	export default {
		name: 'TasksList',
		components: {
			TextField,
			TaskForm,
			Confetti,
			LoadingTasksList,
			TasksListComponent,
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
		}),
		computed: {
			status() {
				return this.$route.meta.status;
			},
		},
		watch: {
			searchText() {
				clearTimeout(this.searchTimeout);
				this.searchTimeout = setTimeout(this.loadTasks, 500);
			},
			'$store.getters.reloadTasks'() {
				this.loadTasks();
			},
		},
		methods: {
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
