<template>
	<teleport to="title">
		Board Test
	</teleport>
	<BaseLayout>
		<template #body>
			<div class="block justify-center">
				<div class="min-h-screen w-full overflow-x-scroll pb-12">
					<div
						style="width: max-content"
					>
						
						<div
							v-for="column in columns"
							:key="column.title"
							class="pr-2"
							style="
								width: 300px;
								display: inline-block;
								height: 100vh;
								float: left;
							"
						>
							<div
								:class="`${$color('blocks')} rounded-lg px-3 py-3 column-width rounded h-full`"
								:style="{
									'background-color': column.status.color
								}"
							>
								<div>
									<p :class="`relative text-white font-semibold font-sans tracking-wide text-sm`">
										{{column.title}}

									<div class="inline-block absolute top-0 right-0">
										<dashboard-dropdown-menu :actions="getActions(column)"></dashboard-dropdown-menu>
									</div>
									</p>
								</div>

								<!-- Draggable component comes from vuedraggable. It provides drag & drop functionality -->
								<draggable v-model="column.tasks" :animation="200" ghost-class="ghost-card" group="tasks" item-key="id" @end="onEnd" :data-status="column.status.id" class="h-full">
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
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Button from "src/components/UIElements/Button";
	import InputField from "components/UIElements/InputField";
	import draggable from "vuedraggable";
	import TaskCard from "./UIElements/TaskCard.vue";
	import DropdownMenu from "components/UIElements/DropdownMenu";
	import DashboardDropdownMenu from "components/UIElements/DashboardDropdownMenu";

	export default {
		name: 'Profile',
		components: {
			DashboardDropdownMenu,
			DropdownMenu,
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
			archivedStatus: null,
			oldColumns: [
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
				}
			],
			columns: []
		}),
		async mounted() {
			await this.loadColumns()
			await this.loadTasks()
		},
		methods: {
			getActions(column) {
				return [
					{
						click: () => {
							alert(column.name);
						},
						label: 'Collapse'
					},
					{
						click: async () => {
							const response = await this.$axios.post(`statuses/${column.status.id}/to/${this.archivedStatus.id}`)
							console.log(response)
							this.loadTasks()
						},
						label: 'Archive all'
					},
					{
						click: async () => {

						},
						label: 'Rename status'
					},
					{
						click: () => {
							alert(column.name);
						},
						label: 'Create task'
					},
				]
			},
			jsonEncode(data) {
				return JSON.stringify(data);
			},
			jsonDecode(stringData) {
				return JSON.parse(stringData)
			},
			async onEnd({to: {dataset: {status}}, item: {dataset: {task}}}) {
				task = this.jsonDecode(task);
				status = parseInt(status);
				if (task.status_id !== status) {
					await this.$axios.put(`tasks/${task.id}/${status}`)
					const foundTask = this.findTask(status, task.id)
					foundTask.status_id = status
				}
				setTimeout(() => this.saveOrders(status), 500)
			},
			async saveOrders(status) {
				const column = this.findColumn(status);
				if (!column) {
					return;
				}
				const orders = [];
				const {tasks} = column
				tasks.forEach(({id}, index) => {
					orders.push({
						id: id,
						order: index + 1
					})
				})

				await this.$axios.put('/tasks/update-orders', {
					tasks: orders
				})
			},
			findTask(status, taskId) {
				const column = this.findColumn(status)
				if (!column) {
					return null;
				}
				const { tasks } = column;
				for (let i = 0; i < tasks.length; ++i) {
					const task = tasks[i];
					if (task.id === taskId) {
						return task;
					}
				}
				return null;
			},
			findColumn(status) {
				const { columns } = this;
				for (let i = 0; i < columns.length; ++i) {
					const column = columns[i];
					if (column.status.id === status) {
						return column
					}
				}
				return null
			},
			onStart(evt) {
			},
			async loadColumns() {
				const columns = []
				const { data: {data: statuses} } = await this.$axios.get(`workspaces/statuses`)
				for (let i = 0; i < statuses.length; ++i) {
					let status = statuses[i];
					if (status.type === 'archived') {
						this.archivedStatus = status
						continue
					}
					columns.push({
						title: status.name,
						status: status,
						tasks: []
					})
				}
				this.$nextTick(() => {
					this.columns = columns
				})
			},
			async loadTasks() {
				for (let i = 0; i < this.columns.length; ++i) {
					let status = this.columns[i].status;
					this.columns[i].tasks = await this.loadTasksByStatus(status);
				}
			},
			async loadTasksByStatus (status) {
				const {data: {data}} = await this.$axios.get(this.getTasksIndexUrl(status.hasOwnProperty('id') ? status.id : status))
				return data.sort((a, b) => {
					if ( a.order < b.order ){
						return -1;
					}
					if ( a.order > b.order ){
						return 1;
					}
					return 0;
				});
			},
			getTasksIndexUrl(status) {
				return `tasks/status/${status}?all&order[column]=order&order[direction]=asc`
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
