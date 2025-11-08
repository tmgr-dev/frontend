<template>
	<div v-if="!task.deleted_at" class="z-10 hidden items-center lg:flex">
		<button
			v-if="showedButtons.done"
			v-tooltip.top="setTooltipData('Done')"
			class="mr-2 rounded bg-blue-700 px-2 py-1 text-white hover:bg-blue-600"
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
		</button>

		<button
			v-if="showedButtons.activate"
			v-tooltip.top="setTooltipData('Reactivate')"
			class="mr-2 rounded bg-purple-700 px-2 py-1 text-white hover:bg-purple-600"
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
		</button>

		<button
			v-if="showedButtons.hide"
			v-tooltip.top="setTooltipData('Hide')"
			class="mr-2 rounded bg-gray-700 px-2 py-1 text-white hover:bg-gray-600"
			@click="$emit('updateStatus', task, 'hidden', `hide-${task.id}`)"
		>
			<span class="relative">
				<span v-if="!isLoadingActions[`hide-${task.id}`]" class="material-icons"
					>visibility_off</span
				>
				<loader v-if="isLoadingActions[`hide-${task.id}`]" is-mini />
			</span>
		</button>

		<button
			v-if="showedButtons.deleteTask"
			v-tooltip.top="setTooltipData('Delete task')"
			class="mr-2 rounded bg-red-700 px-2 py-1 text-white hover:bg-red-600"
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
		</button>
	</div>

	<div v-else class="z-10 hidden items-center lg:flex">
		<button
			v-tooltip.top="setTooltipData('Restore from trash')"
			class="mr-2 rounded bg-green-700 px-2 py-1 text-white hover:bg-green-600"
			@click="restoreTask(task)"
		>
			<span class="material-icons">restore_from_trash</span>
		</button>
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
	import SetTooltipData from '@/mixins/SetTooltipData';
	import Confirm from '@/components/general/Confirm.vue';
	import TaskActionsInTheListMixin from '@/mixins/TaskActionsInTheListMixin';

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
