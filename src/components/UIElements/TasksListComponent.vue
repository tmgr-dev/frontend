<template>
	<div class="w-full items-center justify-center">
		<div v-for="(task, i) in tasks" :key="i" class="w-full px-2 mt-2">
			<div class="shadow-xl rounded-lg md:flex" :class="(task.start_time ? `border-solid border-l-8 border-green-600` : ``)">
				<div class="w-full">
					<div class="p-4 md:p-5" :class="`${$color('blocks')} hover:${$color('blocksHover')}`">
						<div class="flex justify-between items-center">
							<div>
								<p class="font-bold text-xl">
									<span class="cursor-pointer" @click="$router.push(`/tasks/${task.id}/edit`)">{{ task.title }}</span>
									<button v-if="task.category && showCategoryBadges" type="button" class="inline mr-2 bg-gray-700 text-white p-2 rounded ml-5 leading-none items-center" @click="$router.push({name: 'ProjectCategoryChildrenList', params: {id: task.category.id}})">
										{{ task.category.title }}
									</button>
								</p>
								<div class="flex items-start">
									<span>
										<span class="material-icons text-xl" :class="task.start_time ? 'text-green-600' : 'text-orange-600'">alarm</span>
									</span>
									<span class="text-gray-700 ml-2">{{ getTaskFormattedTime(task) }}</span>
								</div>
							</div>
							<DropdownMenu class="lg:hidden" :actions="getActions(task)"></DropdownMenu>
							<div class="hidden lg:flex items-center">
								<new-button @click="$router.push(`/tasks/${task.id}/edit`)" class="mr-2">
									<span class="material-icons">open_in_new</span>
								</new-button>
								<new-button
									v-if="getShowButtons(task).done"
									color="blue"
									@click="updateStatus(task, 'done', `done-${task.id}`)"
									class="mr-2">
										<span class="relative">
											<span class="material-icons bold">done</span>
											<dots-loader v-if="dotsProps[`done-${task.id}`]" />
										</span>
								</new-button>
								<new-button
									v-if="getShowButtons(task).activate"
									color="purple"
									@click="updateStatus(task, 'active', `activate-${task.id}`)"
									class="mr-2">
										<span class="relative">
											<span class="material-icons bold">refresh</span>
											<dots-loader v-if="dotsProps[`activate-${task.id}`]" />
										</span>
								</new-button>
								<new-button
									v-if="getShowButtons(task).hide"
									color="gray"
									@click="updateStatus(task, 'hidden', `hide-${task.id}`)"
									class="mr-2">
										<span class="relative">
											<span class="material-icons">visibility_off</span>
											<dots-loader v-if="dotsProps[`hide-${task.id}`]" />
										</span>
								</new-button>
								<new-button
									v-if="getShowButtons(task).start"
									v-tooltip="'Stop timer'"
									color="red"
									@click="stopCountdown(task, `stop-${task.id}`)"
									class="mr-2">
										<span class="relative">
											<span class="material-icons">alarm_off</span>
											<dots-loader v-if="dotsProps[`stop-${task.id}`]" />
										</span>
								</new-button>

								<new-button
									v-if="getShowButtons(task).stop"
									color="green"
									@click="startCountdown(task, `start-${task.id}`)"
									class="mr-2">
										<span class="relative">
											<span class="material-icons">alarm_on</span>
											<dots-loader v-if="dotsProps[`start-${task.id}`]" />
										</span>
								</new-button>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script>
	import DropdownMenu from "@/components/UIElements/DropdownMenu";
	import DotsLoader from "@/components/UIElements/DotsLoader";

	export default {
		name: "TasksListComponent",
		components: {
			DropdownMenu,
			DotsLoader
		},
		props: {
			tasks: {
				required: false,
				type: Array,
				default: () => []
			},
			status: {
				required: false,
				type: String,
				default: null
			},
			useTaskStatusForButtons: {
				required: false,
				type: Boolean,
				default: false
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true
			},
			dotsProps: {
				required: true,
				type: Object
			}
		},
		methods: {
			async stopCountdown(task, dotId) {
				this.dotsProps[dotId] = true
				await this.$axios.delete(`tasks/${task.id}/countdown`)
				await this.loadTasks()
				this.dotsProps[dotId] = false
			},
			getShowButtons(task) {
				return {
					done: (this.status !== 'done' && !this.useTaskStatusForButtons) || (this.useTaskStatusForButtons && task.status !== 'done'),
					activate: (this.status === 'done' || this.status === 'hidden' && !this.useTaskStatusForButtons) || (this.useTaskStatusForButtons && (task.status === 'done' || task.status === 'hidden')),
					hide: (this.status !== 'hidden' && this.status !== 'done' && !this.useTaskStatusForButtons) || (this.useTaskStatusForButtons && (task.status !== 'hidden' && task.status !== 'done')),
					start: task.start_time,
					stop: !task.start_time && (task.status === 'active' || task.status === 'created')
				}
			},
			async startCountdown(task, dotId) {
				this.dotsProps[dotId] = true
				await this.$axios.post(`tasks/${task.id}/countdown`)
				await this.loadTasks()
				this.dotsProps[dotId] = false
			},
			getTaskFormattedTime(task) {
				const taskTime = task instanceof Object ? task.common_time : task
				let hours = Math.floor(taskTime / 3600)
				let minutes = Math.ceil((taskTime % 3600) / 60)

				return `${hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''}  ${minutes} minute${minutes > 1 ? 's' : ''}`
			},
			async loadTasks() {
				this.$emit('reload-tasks')
			},
			getTasksIndexUrl() {
				if (this.status) {
					return `tasks/status/${this.status}?all`
				}
				return 'tasks/current?all'
			},
			getActions(task) {
				let actions = [
					{
						click: () => {
							this.$router.push(`/tasks/${task.id}/edit`)
						},
						label: 'Edit'
					}
				]

				actions = this.addActionItem(actions, this.getActionItem(task, 'active', 'Activate'), this.getShowButtons(task).activate)
				actions = this.addActionItem(actions, this.getActionItem(task, 'hidden', 'Hide'), this.getShowButtons(task).hide)
				actions = this.addActionItem(actions, this.getActionItem(task, 'done', 'Done'), this.getShowButtons(task).done)

				return actions
			},
			addActionItem(actions, item, show = true) {
				if (!item || !show) {
					return actions
				}
				actions.push(item)
				return actions
			},
			getActionItem(task, status, label) {
				if (status === this.status) {
					return null
				}
				return {
					click: () => {
						this.updateStatus(task, status)
					},
					label: label
				}
			},
			async updateStatus(task, status, dotId) {
				try {
					this.dotsProps[dotId] = true
					await this.$axios.put(`/tasks/${task.id}/${status}`)
					await this.loadTasks()
					this.dotsProps[dotId] = false
				} catch (e) {
					this.dotsProps[dotId] = false
					console.error(e.getMessage())
				}
			},
			capitalize(s) {
				if (typeof s !== 'string') return ''
				return s.charAt(0).toUpperCase() + s.slice(1)
			}
		}
	}
</script>

<style scoped>

</style>
