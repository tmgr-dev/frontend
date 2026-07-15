<template>
	<div v-if="!task.deleted_at" class="z-10 hidden items-center lg:flex">
		<AppTooltip v-if="showedButtons.done" content="Done" side="top">
			<button
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
		</AppTooltip>

		<AppTooltip v-if="showedButtons.activate" content="Reactivate" side="top">
			<button
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
		</AppTooltip>

		<AppTooltip v-if="showedButtons.hide" content="Hide" side="top">
			<button
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
		</AppTooltip>

		<AppTooltip v-if="showedButtons.deleteTask" content="Delete task" side="top">
			<button
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
		</AppTooltip>
	</div>

	<div v-else class="z-10 hidden items-center lg:flex">
		<AppTooltip content="Restore from trash" side="top">
			<button
				class="mr-2 rounded bg-green-700 px-2 py-1 text-white hover:bg-green-600"
				@click="restoreTask(task)"
			>
				<span class="material-icons">restore_from_trash</span>
			</button>
		</AppTooltip>
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
	import Confirm from '@/components/general/Confirm.vue';
	import AppTooltip from '@/components/general/AppTooltip.vue';
	import TaskActionsInTheListMixin from '@/mixins/TaskActionsInTheListMixin';

	export default {
		name: 'TaskButtonsInTheList',
		emits: ['updateStatus', 'stopCountdown', 'startCountdown'],
		components: {
			Confirm,
			AppTooltip,
		},
		mixins: [TaskActionsInTheListMixin],
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
