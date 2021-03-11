<template>
	<div class="tc-hidden lg:flex items-center z-10" v-if="!task.deleted_at">
		<new-button
			@click="$router.push(`/${task.id}/edit`)"
			class="mr-2"
			v-tooltip.top="userSettings.showTooltips ? 'Open' : { visible: false }">
			<span class="material-icons">open_in_new</span>
		</new-button>

		<new-button
			v-if="showedButtons.done"
			v-tooltip.top="userSettings.showTooltips ? 'Done' : { visible: false }"
			color="blue"
			@click="$emit('updateStatus', task, 'done', `done-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons text-bold" v-if="!isLoadingActions[`done-${task.id}`]">done</span>
				<loader v-if="isLoadingActions[`done-${task.id}`]" :is-mini="true" :is-static="true" />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.activate"
			v-tooltip.top="userSettings.showTooltips ? 'Reactivate' : { visible: false }"
			color="purple"
			@click="$emit('updateStatus', task, 'active', `activate-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons text-bold" v-if="!isLoadingActions[`activate-${task.id}`]">refresh</span>
				<loader v-if="isLoadingActions[`activate-${task.id}`]" :is-mini="true" :is-static="true" />
			</span>
		</new-button>
		
		<new-button
			v-if="showedButtons.hide"
			v-tooltip.top="userSettings.showTooltips ? 'Hide' : { visible: false }"
			color="gray"
			@click="$emit('updateStatus', task, 'hidden', `hide-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`hide-${task.id}`]">visibility_off</span>
				<loader v-if="isLoadingActions[`hide-${task.id}`]" :is-mini="true" :is-static="true" />
			</span>
		</new-button>
		
		<new-button
			v-if="showedButtons.start"
			v-tooltip.top="userSettings.showTooltips ? 'Stop timer' : { visible: false }"
			color="red"
			@click="$emit('stopCountdown', task, `stop-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`stop-${task.id}`]">alarm_off</span>
				<loader v-if="isLoadingActions[`stop-${task.id}`]" :is-mini="true" :is-static="true" />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.stop"
			v-tooltip.top="userSettings.showTooltips ? 'Start timer' : { visible: false }"
			color="green"
			@click="$emit('startCountdown', task, `start-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`start-${task.id}`]">alarm_on</span>
				<loader v-if="isLoadingActions[`start-${task.id}`]" :is-mini="true" :is-static="true" />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.deleteTask"
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
	
	<div class="tc-hidden lg:flex items-center z-10" v-else>
		<new-button
			@click="restoreTask(task)"
			class="mr-2"
			v-tooltip.top="userSettings.showTooltips ? 'Restore from trash' : { visible: false }">
			<span class="material-icons">restore_from_trash</span>
		</new-button>
	</div>

	<confirm
		v-if="confirm"
		:title="confirm.title"
		:body="confirm.body"
		@onOk="confirm.action()"
		@onCancel="confirm = undefined"
	/>
</template>

<script>
	import Confirm from "src/components/UIElements/Confirm";
	import TaskActionsInTheListMixin from "src/mixins/TaskActionsInTheListMixin";

	export default {
		name: 'TaskButtonsInTheList',
		emits: [
			'updateStatus',
			'stopCountdown',
			'startCountdown'
		],
		components: {
			Confirm
		},
		mixins: [
			TaskActionsInTheListMixin
		],
		props: {
			showedButtons: {
				type: Object,
				required: true
			},
			task: {
				type: Object,
				required: true,
			},
			isLoadingActions: {
				type: Object,
				required: true
			}
		},
		data: () => ({
			confirm: null
		}),
		computed: {
			userSettings () {
				return this.$store.getters.getUserSettings || {}
			}
		},
		methods: {
			showConfirm (title, body, action) {
				this.confirm = { title, body, action }
			}
		}
	}
</script>
