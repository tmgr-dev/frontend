<template>
	<div class="draggable-block">
		<vue-draggable-resizable
			:resizable="false"
			:w="250"
			class-name="rounded bg-green-600 py-5 px-2"
		>
			<slot></slot>
			<a
				class="absolute right-0 top-0 pr-1 pt-1 text-gray-400 hover:text-gray-100"
				href="#close"
				@click.prevent="$emit('close')"
			>
				<span class="material-icons text-bold">close</span>
			</a>
			<div class="mt-2 flex w-full flex-wrap justify-center">
				<button
					v-tooltip.top="setTooltipData('Done')"
					class="mr-1 mt-2 rounded bg-green-700 py-2 text-white hover:bg-green-800"
					@click="$emit('updateStatus', 'done')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('done')"
							class="material-icons text-bold"
							>done</span
						>
						<loader v-if="isLoadingActions.includes('done')" is-mini />
					</span>
				</button>
				<button
					v-tooltip.top="setTooltipData('Archive')"
					class="mr-1 mt-2 rounded bg-red-700 py-2 text-white hover:bg-red-800"
					@click="$emit('updateStatus', '15')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('15')"
							class="material-icons text-bold"
							>archive</span
						>
						<loader v-if="isLoadingActions.includes('15')" is-mini />
					</span>
				</button>
				<button
					v-tooltip.top="setTooltipData('Reactivate')"
					class="mr-1 mt-2 rounded bg-purple-700 py-2 text-white hover:bg-purple-600"
					@click="$emit('updateStatus', 'active')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('active')"
							class="material-icons text-bold"
							>refresh</span
						>
						<loader v-if="isLoadingActions.includes('active')" is-mini />
					</span>
				</button>
				<button
					v-tooltip.top="setTooltipData('Hide')"
					class="mr-1 mt-2 rounded bg-gray-700 py-2 text-white hover:bg-gray-600"
					@click="$emit('updateStatus', 'hidden')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('hidden')"
							class="material-icons"
							>visibility_off</span
						>
						<loader v-if="isLoadingActions.includes('hidden')" is-mini />
					</span>
				</button>
				<button
					v-if="status === 'hidden' || status === 'done'"
					v-tooltip.top="setTooltipData('Delete')"
					class="mt-2 rounded bg-red-700 py-2 text-white hover:bg-red-600"
					@click="$emit('remove')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('delete')"
							class="material-icons"
							>delete</span
						>
						<loader v-if="isLoadingActions.includes('delete')" is-mini />
					</span>
				</button>
				<button
					v-tooltip.top="setTooltipData('Export to CSV')"
					class="mt-2 rounded bg-blue-700 py-2 text-white hover:bg-blue-600"
					@click="$emit('export')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('csv')"
							class="material-icons"
							>description</span
						>
						<loader v-if="isLoadingActions.includes('csv')" is-mini />
					</span>
				</button>
				<button
					v-tooltip.top="setTooltipData('Export to JPEG')"
					class="ml-1 mt-2 rounded bg-blue-700 py-2 text-white hover:bg-blue-600"
					@click="$emit('export', 'jpg')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('jpg')"
							class="material-icons"
							>image</span
						>
						<loader v-if="isLoadingActions.includes('jpg')" is-mini />
					</span>
				</button>
				<button
					v-tooltip.top="setTooltipData('Export to XLSX')"
					class="ml-1 mt-2 rounded bg-blue-700 py-2 text-white hover:bg-blue-600"
					@click="$emit('export', 'xlsx')"
				>
					<span class="relative">
						<span
							v-if="!isLoadingActions.includes('xlsx')"
							class="material-icons"
							>list</span
						>
						<loader v-if="isLoadingActions.includes('xlsx')" is-mini />
					</span>
				</button>
			</div>
		</vue-draggable-resizable>
	</div>
</template>

<script>
	import SetTooltipData from '@/mixins/SetTooltipData';
	import VueDraggableResizable from '@/plugins/VueDraggableResizable/VueDraggableResizable.vue';

	export default {
		name: 'TasksMultipleActionsModal',
		emits: ['export', 'remove', 'updateStatus'],
		mixins: [SetTooltipData],
		components: {
			VueDraggableResizable,
		},
		props: {
			status: {
				required: false,
				type: String,
				default: null,
			},
			isLoadingActions: {
				type: Object,
				required: true,
			},
		},
	};
</script>

<style scoped>
	button {
		width: 45px;
		height: 40px;
	}

	.loader {
		margin: auto;
	}
</style>
