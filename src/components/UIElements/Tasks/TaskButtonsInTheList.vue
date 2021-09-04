<template>
	<div class="tc-hidden lg:flex items-center z-10" v-if="!task.deleted_at">
		<new-button
			@click="$store.commit('currentTaskIdForModal', task.id)"
			class="mr-2"
			v-tooltip.top="setTooltipData('Open')">
			<span class="material-icons">open_in_new</span>
		</new-button>

		<new-button
			v-if="showedButtons.done"
			v-tooltip.top="setTooltipData('Done')"
			color="blue"
			@click="$emit('updateStatus', task, 'done', `done-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons text-bold" v-if="!isLoadingActions[`done-${task.id}`]">done</span>
				<loader v-if="isLoadingActions[`done-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.activate"
			v-tooltip.top="setTooltipData('Reactivate')"
			color="purple"
			@click="$emit('updateStatus', task, 'active', `activate-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons text-bold" v-if="!isLoadingActions[`activate-${task.id}`]">refresh</span>
				<loader v-if="isLoadingActions[`activate-${task.id}`]" is-mini />
			</span>
		</new-button>
		
		<new-button
			v-if="showedButtons.hide"
			v-tooltip.top="setTooltipData('Hide')"
			color="gray"
			@click="$emit('updateStatus', task, 'hidden', `hide-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`hide-${task.id}`]">visibility_off</span>
				<loader v-if="isLoadingActions[`hide-${task.id}`]" is-mini />
			</span>
		</new-button>
		
		<new-button
			v-if="showedButtons.start"
			v-tooltip.top="setTooltipData('Stop timer')"
			color="red"
			@click="$emit('stopCountdown', task, `stop-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`stop-${task.id}`]">alarm_off</span>
				<loader v-if="isLoadingActions[`stop-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.stop"
			v-tooltip.top="setTooltipData('Start timer')"
			color="green"
			@click="$emit('startCountdown', task, `start-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`start-${task.id}`]">alarm_on</span>
				<loader v-if="isLoadingActions[`start-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.deleteTask"
			v-tooltip.top="setTooltipData('Delete task')"
			color="red"
			@click="deleteTask(task, `delete-${task.id}`)"
			class="mr-2">
			<span class="relative">
				<span class="material-icons" v-if="!isLoadingActions[`delete-${task.id}`]">delete</span>
				<loader v-if="isLoadingActions[`delete-${task.id}`]" is-mini />
			</span>
		</new-button>
	</div>
	
	<div class="tc-hidden lg:flex items-center z-10" v-else>
		<new-button
			@click="restoreTask(task)"
			class="mr-2"
			v-tooltip.top="setTooltipData('Restore from trash')">
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
	import SetTooltipData from "src/mixins/SetTooltipData";
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
			SetTooltipData,
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
		methods: {
			showConfirm (title, body, action) {
				this.confirm = { title, body, action }
			}
		}
	}
</script>
