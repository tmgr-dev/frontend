<template>
	<teleport to="title">
		Board Test
	</teleport>
	<BaseLayout>
		<template #header>
			Board Test
		</template>
		<template #body>
			<div class="flex justify-center">
				<div class="min-h-screen flex w-full py-12">
					<div
						v-for="column in columns"
						:key="column.title"
						class="w-1/3 pr-2"
					>
						<div
							:class="`${$color('blocks')} rounded-lg px-3 py-3 column-width rounded`"
						>
							<p :class="`${$color('textMain')} font-semibold font-sans tracking-wide text-sm`">{{column.title}}</p>
							<!-- Draggable component comes from vuedraggable. It provides drag & drop functionality -->
							<draggable v-model="column.tasks" :animation="200" ghost-class="ghost-card" group="tasks" item-key="id" @end="onEnd" :data-status="column.status">
								<template #item="{element: task}">
									<task-card
										:task="task"
										class="mt-3 cursor-move"
										:data-task="jsonEncode(task)"
									></task-card>
								</template>
							</draggable>
						</div>
					</div>
				</div>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Button from "src/components/UIElements/Button";
	import InputField from "components/UIElements/InputField";
	import draggable from "vuedraggable";
	import TaskCard from "./UIElements/TaskCard.vue";

	export default {
		name: 'Profile',
		components: {
			InputField,
			Button,
			TaskCard,
			draggable
		},
		data: () => ({
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			errors: {},
			columns: [
				{
					title: "Backlog",
					status: "created",
					tasks: []
				},
				{
					title: "In Progress",
					status: "active",
					tasks: []
				},
				{
					title: "Hidden",
					status: "hidden",
					tasks: []
				},
				// {
				// 	title: "Archive",
				// 	status: "archive",
				// 	tasks: []
				// }
			]
		}),
		async mounted() {
			this.loadTasks()
		},
		methods: {
			jsonEncode(data) {
				return JSON.stringify(data);
			},
			jsonDecode(stringData) {
				return JSON.parse(stringData)
			},
			async onEnd({to: {dataset: {status}}, item: {dataset: {task}}}) {
				task = this.jsonDecode(task);
				await this.$axios.put(`tasks/${task.id}/${status}`)
				const foundTask = this.findTask(task.id)
				foundTask.status = status
			},
			findTask(taskId) {
				const { columns } = this;
				for (let i = 0; i < columns.length; ++i) {
					const {tasks} = columns[i];
					for (let j = 0; j < tasks.length; ++j) {
						const task = tasks[j];
						if (task.id === taskId) {
							return task
						}
					}
				}
				return null
			},
			onStart(evt) {
				console.log(evt)
			},
			async loadTasks() {
				this.columns.forEach(async ({status}, index) => {
					this.columns[index].tasks = await this.loadTasksByStatus(status)
					console.log(this.columns[index].tasks)
				})
			},
			async loadTasksByStatus (status) {
				const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl(status))
				return data;
			},
			getTasksIndexUrl(status) {
				return `tasks/status/${status}?all`
			},
			async saveUser() {
				try {
					const {data: {data}} = await this.$axios.put('user', this.user)
					this.user = data
					this.showAlert('Saved', 'User data saved')
				} catch ({response: {data: { errors }}}) {
					this.errors = errors
				}
			}
		}
	}
</script>

<style scoped>
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}
</style>
