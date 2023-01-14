<template>
	<div>
		<teleport to="title">
			{{ h1[$route.name] }}
		</teleport>
		<BaseLayout>
			<template #action>
				<div class="flex justify-between flex-wrap">
					<transition name="fade">
						<div
							v-if="summaryTimeString"
							class="sm:w-auto w-full text-opacity-25 text-center text-bold lg:text-2xl sm:text-xl text-lg">
							{{ summaryTimeString }}
						</div>
					</transition>

					<div class="sm:ml-auto sm:mt-0 ml-0 sm:w-auto w-full text-center mt-2">
						<a class="pr-1" href="#" title="Search tasks" @click.prevent="showSearchInput = !showSearchInput">
						<span
							class="material-icons sm:text-4xl text-3xl text-gray-700 opacity-75 hover:opacity-100">{{ showSearchInput ? 'search_off' : 'search'
							}}</span>
						</a>
						<a class="pr-1" href="#" title="Select all" @click.prevent="selectAll()">
							<span class="material-icons sm:text-4xl text-3xl text-gray-700 opacity-75 hover:opacity-100">done_all</span>
						</a>
						<a href="#" title="Add Task" @click="$store.dispatch('showCreateTaskModal')">
							<span class="material-icons sm:text-4xl text-3xl text-gray-700 opacity-75 hover:opacity-100">add_circle_outline</span>
						</a>
					</div>
				</div>
			</template>
			<template #body>
				<transition mode="out-in" name="fade">
					<div>
						<input-field v-if="showSearchInput" v-model="searchText" class="px-2 pb-5" placeholder="Enter task name"
												 @keydown:enter="loadTasks"></input-field>
					</div>
				</transition>
				<tasks-list-component
					v-if="tasks && tasks.length > 0"
					ref="tasksListComponent"
					:is-loading-actions="isLoadingActions"
					:status="status"
					:tasks="tasks"
					has-selectable
					@reload-tasks="reloadTasks"
				/>
				<div v-else-if="errorLoading" class="text-center italic text-xl">
					Something went wrong...
				</div>
				<div v-else-if="!showLoader" class="text-center italic text-xl">
					You don't have tasks here

					<confetti v-if="hasAbilityToShowConfetti" />
				</div>
				<loading-tasks-list v-if="showLoader" class="mx-2" />
			</template>
		</BaseLayout>
	</div>
</template>

<script>
import TasksListMixin from 'src/mixins/TasksListMixin';
import LoadingButtonActions from 'src/mixins/LoadingButtonActions';
import LoadingTasksList from 'components/UIElements/Tasks/LoadingTasksList';
import TasksListComponent from 'src/components/UIElements/Tasks/TasksListComponent';
import Confetti from 'components/UIElements/Confetti';
import FullscreenModal from 'components/Layouts/FullscreenModal';
import TaskForm from 'components/Tasks/TaskForm';

export default {
	name: 'TasksList',
	components: {
		TaskForm,
		FullscreenModal,
		Confetti,
		LoadingTasksList,
		TasksListComponent
	},
	mixins: [
		LoadingButtonActions,
		TasksListMixin
	],
	data: () => ({
		showCreateTaskForm: false,
		errorLoading: false,
		showSearchInput: false,
		panel: false,
		searchText: null,
		searchTimeout: null,
		summaryTimeString: null,
		showLoader: true,
		h1: {
			CurrentTasksList: 'Current tasks',
			HiddenTasksList: 'Hidden tasks',
			ArchiveTasksList: 'Archive tasks'
		},
		tasks: [],
		isLoadingActions: {},
		hasAbilityToShowConfetti: false
	}),
	computed: {
		status() {
			return this.$route.meta.status;
		}
	},
	watch: {
		searchText() {
			clearTimeout(this.searchTimeout);
			this.searchTimeout = setTimeout(this.loadTasks, 1000);

		}
	},
	methods: {
		reloadTasks() {
			this.hasAbilityToShowConfetti = true;
			this.loadTasks();
		},
		async loadTasks() {
			try {
				clearTimeout(this.searchTimeout);
				const { searchText } = this;
				const { data: { data } } = await this.$axios.get(this.tasksIndexUrl + (searchText ? '&search=' + searchText : ''));
				this.summaryTimeString = this.getTaskFormattedTime(data.reduce((summary, task) => task.common_time + summary, 0));
				this.tasks = data;
				return data;
			} catch (e) {
				console.error(e);
				this.errorLoading = true;
			} finally {
				this.showLoader = false;
			}
		},
		selectAll() {
			if (!this.$refs.tasksListComponent) {
				return;
			}
			this.$refs.tasksListComponent.selectAll();
		}
	},
	async created() {
		const data = await this.loadTasks();
		this.setLoadingActions(data);
	}
};
</script>
