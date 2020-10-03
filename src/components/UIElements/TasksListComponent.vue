<template>
	<div class="w-full items-center justify-center">
		<div v-selectable="{ selectedGetter, selectedSetter, selectingSetter }">
			<div class="selection"></div>

			<div
				v-for="(task, i) in tasks"
				:key="i"
				:class="{ selected: !!selected[i], selecting: !!selecting[i] }"
				class="w-full px-2 mt-2 selectable">
				<div class="shadow-xl rounded-lg md:flex" :class="(task.start_time ? `border-solid border-l-8 border-green-600` : ``)">
					<div class="w-full">
						<div class="p-4 md:p-5" :class="`${$color('blocks')} hover:${$color('blocksHover')}`">
							<div class="flex justify-between items-center">
								<div>
									<div>
										<router-link :to="`/tasks/${task.id}/edit`" class="font-bold text-xl">
											{{ task.title }}
										</router-link>
										<router-link
											v-if="task.category && showCategoryBadges"
											tag="button"
											:to="{name: 'ProjectCategoryChildrenList', params: {id: task.category.id}}"
											class="inline bg-gray-700 text-white py-1 px-2 rounded ml-2 leading-none text-base">
											{{ task.category.title }}
										</router-link>
									</div>
									<div class="flex items-start">
										<span>
											<span class="material-icons text-xl" :class="task.start_time ? 'text-green-600' : 'text-orange-600'">alarm</span>
										</span>
										<span class="text-gray-700 ml-2">{{ getTaskFormattedTime(task) }}</span>
									</div>
								</div>
								<DropdownMenu class="lg:hidden" :actions="getActions(task)"></DropdownMenu>
								<div class="hidden lg:flex items-center">
									<new-button
										@click="$router.push(`/tasks/${task.id}/edit`)"
										class="mr-2"
										v-tooltip.top="userSettings.showTooltips ? 'Open' : { visible: false }">
										<span class="material-icons">open_in_new</span>
									</new-button>

									<new-button
										v-if="getShowButtons(task).done"
										v-tooltip.top="userSettings.showTooltips ? 'Done' : { visible: false }"
										color="blue"
										@click="updateStatus(task, 'done', `done-${task.id}`)"
										class="mr-2">
											<span class="relative">
												<span class="material-icons text-bold" v-if="!isLoadingActions[`done-${task.id}`]">done</span>
												<loader v-if="isLoadingActions[`done-${task.id}`]" :is-mini="true" :is-static="true" />
											</span>
									</new-button>
									<new-button
										v-if="getShowButtons(task).activate"
										v-tooltip.top="userSettings.showTooltips ? 'Reactivate' : { visible: false }"
										color="purple"
										@click="updateStatus(task, 'active', `activate-${task.id}`)"
										class="mr-2">
											<span class="relative">
												<span class="material-icons text-bold" v-if="!isLoadingActions[`activate-${task.id}`]">refresh</span>
												<loader v-if="isLoadingActions[`activate-${task.id}`]" :is-mini="true" :is-static="true" />
											</span>
									</new-button>
									<new-button
										v-if="getShowButtons(task).hide"
										v-tooltip.top="userSettings.showTooltips ? 'Hide' : { visible: false }"
										color="gray"
										@click="updateStatus(task, 'hidden', `hide-${task.id}`)"
										class="mr-2">
											<span class="relative">
												<span class="material-icons" v-if="!isLoadingActions[`hide-${task.id}`]">visibility_off</span>
												<loader v-if="isLoadingActions[`hide-${task.id}`]" :is-mini="true" :is-static="true" />
											</span>
									</new-button>
									<new-button
										v-if="getShowButtons(task).start"
										v-tooltip.top="userSettings.showTooltips ? 'Stop timer' : { visible: false }"
										color="red"
										@click="stopCountdown(task, `stop-${task.id}`)"
										class="mr-2">
											<span class="relative">
												<span class="material-icons" v-if="!isLoadingActions[`stop-${task.id}`]">alarm_off</span>
												<loader v-if="isLoadingActions[`stop-${task.id}`]" :is-mini="true" :is-static="true" />
											</span>
									</new-button>

									<new-button
										v-if="getShowButtons(task).stop"
										v-tooltip.top="userSettings.showTooltips ? 'Start timer' : { visible: false }"
										color="green"
										@click="startCountdown(task, `start-${task.id}`)"
										class="mr-2">
											<span class="relative">
												<span class="material-icons" v-if="!isLoadingActions[`start-${task.id}`]">alarm_on</span>
												<loader v-if="isLoadingActions[`start-${task.id}`]" :is-mini="true" :is-static="true" />
											</span>
									</new-button>

									<new-button
										v-if="status === 'hidden' || status === 'done'"
										v-tooltip.top="userSettings.showTooltips ? 'Delete task' : { visible: false }"
										color="red"
										@click="deleteTask(task, `delete-${task.id}`)"
										class="mr-2">
											<span class="relative">
												<span class="material-icons" v-if="!isLoadingActions[`delete-${task.id}`]">delete</span>
												<loader v-if="isLoadingActions[`delete-${task.id}`]" :is-mini="true" :is-static="true" />
											</span>
									</new-button>
								</div>
							</div>
						</div>
					</div>
				</div>
			</div>

			<modal
				v-if="showTimeInModal"
				:is-center="true"
				:modal-width="300"
				@close="closeTimeInModal">
				<template #modal-body>
					<div class="time-for-modal" v-if="timeForModal">
						{{ timeForModal }}
					</div>
					<div class="time-for-modal" v-else>
						Ooops...
					</div>

					<div class="text-center">
						<button
							type="button"
							@click="closeTimeInModal"
							class="w-1/4 bg-green-700 text-white p-2 rounded mt-2 hover:bg-green-600">
							{{ timeForModal ? 'OK!' : 'Sorry' }}
						</button>
					</div>
				</template>
			</modal>
		</div>
	</div>
</template>

<script>
	import DropdownMenu from "@/components/UIElements/DropdownMenu";
	import TasksListMixin from "@/mixins/TasksListMixin";
	import selectable from 'vue-selectable';

	export default {
		name: "TasksListComponent",
		components: {
			DropdownMenu
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
			isLoadingActions: {
				required: true,
				type: Object
			}
		},
		mixins: [ TasksListMixin ],
		directives: { selectable },
		data: () => ({
			selected: [],
			selecting: [],
			showTimeInModal: false,
			timeForModal: null
		}),
		computed: {
			userSettings () {
				return this.$store.getters.getUserSettings
			}
		},
		methods: {
			async stopCountdown(task, dotId) {
				this.isLoadingActions[dotId] = true
				await this.$axios.delete(`tasks/${task.id}/countdown`)
				await this.loadTasks()
				this.isLoadingActions[dotId] = false
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
				this.isLoadingActions[dotId] = true
				await this.$axios.post(`tasks/${task.id}/countdown`)
				await this.loadTasks()
				this.isLoadingActions[dotId] = false
			},
			async loadTasks() {
				await this.$emit('reload-tasks')
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
					this.isLoadingActions[dotId] = true
					await this.$axios.put(`/tasks/${task.id}/${status}`)
					await this.loadTasks()
					this.isLoadingActions[dotId] = false
				} catch (e) {
					this.isLoadingActions[dotId] = false
					console.error(e)
				}
			},
			async deleteTask (task, dotId) {
				try {
					this.isLoadingActions[dotId] = true
					await this.$axios.delete(`/tasks/${task.id}`)
					await this.loadTasks()
					this.isLoadingActions[dotId] = false
				} catch (e) {
					this.isLoadingActions[dotId] = false
					console.error(e)
				}
			},
			capitalize(s) {
				if (typeof s !== 'string') return ''
				return s.charAt(0).toUpperCase() + s.slice(1)
			},
			selectedGetter () {
				return this.selected;
			},
			selectedSetter (v) {
				this.selected = v;
				if (this.selected.includes(true)) {
					const tasks = this.tasks.filter((task, index) => this.selected[index])
					const time = tasks.reduce((p, c) => p + c.common_time, 0)
					this.showTimeInModal = true
					this.timeForModal = this.formatTime(time)
				}
			},
			closeTimeInModal () {
				this.selected = []
				this.showTimeInModal = false
			},
			selectingSetter (v) {
				this.selecting = v;
			}
		}
	}
</script>

<style scoped>
	.time-for-modal {
		font-size: 20px;
		color: #3c3c3c;
		font-weight: bold;
		text-align: center;
	}
	.selection {
		position: absolute;
		border: 1px dotted #000;
		z-index: 9999;
		top: 0;
		left: 0;
		cursor: default;
		display: none;
	}
	.selectable {
		-webkit-touch-callout: none;
		-webkit-user-select: none;
		-moz-user-select: none;
		-ms-user-select: none;
		user-select: none;
	}
	.selectable.selecting {
		background-color: yellow;
	}
	.selectable.selected {
		background-color: orange;
	}
</style>
