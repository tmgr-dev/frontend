<template>
	<BaseLayout>
		<template #header>
			{{ h1[$route.name] }}
			<br>
			<small v-if="summaryTimeString" class="text-opacity-25">{{ summaryTimeString }}</small>
		</template>
		<template #action>
			<div class="left-0 bottom-0 mr-4 absolute">
			</div>
			<div class="md:absolute md:right-0 md:bottom-0 text-center md:text-right">
				<a href="#" @click.prevent="selectAll()" title="Select all" class="pr-1">
					<span class="material-icons text-4xl text-gray-700 opacity-75 hover:opacity-100">done_all</span>
				</a>
				<router-link :to="`/tasks/create`" title="Add Task" class="pr-5">
					<span class="material-icons text-4xl text-gray-700 opacity-75 hover:opacity-100">add_circle_outline</span>
				</router-link>
			</div>
		</template>
		<template #body>
			<default-tasks-list-component
				v-if="tasks && tasks.length > 0 && showDefaultList"
				:tasks="tasks"
				:status="status"
				ref="tasksListComponent"
			/>
			<tasks-list-component
				v-else-if="tasks && tasks.length > 0 && !showDefaultList"
				:tasks="tasks"
				:status="status"
				:is-loading-actions="isLoadingActions"
				@reload-tasks="loadTasks"
				ref="tasksListComponent"
			/>
			<div v-else-if="!showLoader" style="font-style: italic; font-size: 18px;" class="text-center">
				You don't have tasks here
			</div>
			<loader v-if="showLoader" :is-active="true" style="margin-top: 2rem" />
		</template>
	</BaseLayout>
</template>

<script>
	import TasksListComponent from "../UIElements/TasksListComponent";
	import DefaultTasksListComponent from "@/components/UIElements/DefaultTasksListComponent";
	import LoadingButtonActions from "@/mixins/LoadingButtonActions";
	import TasksListMixin from "@/mixins/TasksListMixin";

	export default {
		name: 'TasksList',
		components: {
			TasksListComponent,
			DefaultTasksListComponent
		},
		mixins: [ LoadingButtonActions, TasksListMixin ],
		data() {
			return {
				panel: false,
				showDefaultList: false,
				summaryTimeString: null,
				showLoader: true,
				h1: {
					CurrentTasksList: 'Current tasks',
					HiddenTasksList: 'Hidden tasks',
					ArchiveTasksList: 'Archive tasks'
				},
				tasks: [],
				isLoadingActions: {}
			}
		},
		computed: {
			status() {
				return this.$route.meta.status
			}
		},
		async created() {
			const data = await this.loadTasks()
			this.setLoadingActions(data)
		},
		methods: {
			async loadTasks() {
				const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl())
				this.summaryTimeString = this.getTaskFormattedTime(data.reduce((summary, task) => task.common_time + summary, 0))
				this.tasks = data
				this.showLoader = false
				return data
			},
			selectAll () {
				if (!this.$refs.tasksListComponent) {
					return
				}
				this.$refs.tasksListComponent.selectAll()
			}
		}
	}
</script>
