<template>
	<div v-if="!task.deleted_at" class="z-10 hidden items-center lg:flex">
		<new-button
			v-tooltip.top="setTooltipData('Open')"
			class="mr-2"
			@click="$store.commit('setCurrentTaskIdForModal', task.id)"
		>
			<span class="material-icons">open_in_new</span>
		</new-button>

		<new-button
			v-if="showedButtons.done"
			v-tooltip.top="setTooltipData('Done')"
			class="mr-2"
			button-class="bg-blue-700 hover:bg-blue-600"
			@click="$emit('updateStatus', task, 'done', `done-${task.id}`)"
		>
			<span class="relative">
				<span
					v-if="!isLoadingActions[`done-${task.id}`]"
					class="material-icons text-bold"
					>done</span
				>
				<loader v-if="isLoadingActions[`done-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.activate"
			v-tooltip.top="setTooltipData('Reactivate')"
			class="mr-2"
			button-class="bg-purple-700 hover:bg-purple-600"
			@click="$emit('updateStatus', task, 'active', `activate-${task.id}`)"
		>
			<span class="relative">
				<span
					v-if="!isLoadingActions[`activate-${task.id}`]"
					class="material-icons text-bold"
					>refresh</span
				>
				<loader v-if="isLoadingActions[`activate-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.hide"
			v-tooltip.top="setTooltipData('Hide')"
			class="mr-2"
			button-class="bg-gray-700 hover:bg-gray-600"
			@click="$emit('updateStatus', task, 'hidden', `hide-${task.id}`)"
		>
			<span class="relative">
				<span v-if="!isLoadingActions[`hide-${task.id}`]" class="material-icons"
					>visibility_off</span
				>
				<loader v-if="isLoadingActions[`hide-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.start"
			v-tooltip.top="setTooltipData('Stop timer')"
			class="mr-2"
			button-class="bg-red-700 hover:bg-red-600"
			@click="$emit('stopCountdown', task, `stop-${task.id}`)"
		>
			<span class="relative">
				<span v-if="!isLoadingActions[`stop-${task.id}`]" class="material-icons"
					>alarm_off</span
				>
				<loader v-if="isLoadingActions[`stop-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.stop"
			v-tooltip.top="setTooltipData('Start timer')"
			class="mr-2"
			button-class="bg-green-700 hover:bg-green-600"
			@click="$emit('startCountdown', task, `start-${task.id}`)"
		>
			<span class="relative">
				<span
					v-if="!isLoadingActions[`start-${task.id}`]"
					class="material-icons"
					>alarm_on</span
				>
				<loader v-if="isLoadingActions[`start-${task.id}`]" is-mini />
			</span>
		</new-button>

		<new-button
			v-if="showedButtons.deleteTask"
			v-tooltip.top="setTooltipData('Delete task')"
			class="mr-2"
			button-class="bg-red-700 hover:bg-red-600"
			@click="removeTask(task, `delete-${task.id}`)"
		>
			<span class="relative">
				<span
					v-if="!isLoadingActions[`delete-${task.id}`]"
					class="material-icons"
					>delete</span
				>
				<loader v-if="isLoadingActions[`delete-${task.id}`]" is-mini />
			</span>
		</new-button>
	</div>

	<div v-else class="z-10 hidden items-center lg:flex">
		<new-button
			v-tooltip.top="setTooltipData('Restore from trash')"
			class="mr-2"
			@click="restoreTask(task)"
		>
			<span class="material-icons">restore_from_trash</span>
		</new-button>
	</div>

	<Transition name="fade">
		<confirm
			v-if="confirm"
			:body="confirm.body"
			:title="confirm.title"
			@onCancel="confirm = undefined"
			@onOk="confirm.action()"
		/>
	</Transition>
</template>

<script>
	import SetTooltipData from 'src/mixins/SetTooltipData';
	import Confirm from 'src/components/general/Confirm.vue';
	import TaskActionsInTheListMixin from 'src/mixins/TaskActionsInTheListMixin';

	export default {
		name: 'TaskButtonsInTheList',
		emits: ['updateStatus', 'stopCountdown', 'startCountdown'],
		components: {
			Confirm,
		},
		mixins: [SetTooltipData, TaskActionsInTheListMixin],
		props: {
			showedButtons: {
				type: Object,
				required: true,
			},
			task: {
				type: Object,
				required: true,
			},
			isLoadingActions: {
				type: Object,
				required: true,
			},
		},
		data: () => ({
			confirm: null,
		}),
		methods: {
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
		},
	};
</script>
