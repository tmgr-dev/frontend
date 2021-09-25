<template>
	<div class="draggable-block">
		<vue-draggable-resizable :resizable="false" :w="250" class-name="rounded bg-green-600 py-5 px-2">
			<slot></slot>
			<a href="#close"
				 class="absolute top-0 right-0 pr-1 pt-1 text-gray-400 hover:text-gray-100"
				 @click.prevent="$emit('close')">
				<span class="material-icons text-bold">close</span>
			</a>
			<div class="w-full flex flex-wrap justify-center mt-2">
				<button
					@click="$emit('updateStatus', 'done')"
					v-tooltip.top="setTooltipData('Done')"
					class="mr-1 bg-green-700 text-white rounded py-2 mt-2 hover:bg-green-800">
					<span class="relative">
						<span class="material-icons text-bold" v-if="!isLoadingActions.includes('done')">done</span>
						<loader v-if="isLoadingActions.includes('done')" is-mini />
					</span>
				</button>
				<button
					@click="$emit('updateStatus', '15')"
					v-tooltip.top="setTooltipData('Archive')"
					class="mr-1 bg-red-700 text-white rounded py-2 mt-2 hover:bg-red-800">
					<span class="relative">
						<span class="material-icons text-bold" v-if="!isLoadingActions.includes('15')">archive</span>
						<loader v-if="isLoadingActions.includes('15')" is-mini />
					</span>
				</button>
				<button
					@click="$emit('updateStatus', 'active')"
					v-tooltip.top="setTooltipData('Reactivate')"
					class="mr-1 bg-purple-700 text-white rounded py-2 mt-2 hover:bg-purple-600">
					<span class="relative">
						<span class="material-icons text-bold" v-if="!isLoadingActions.includes('active')">refresh</span>
						<loader v-if="isLoadingActions.includes('active')" is-mini />
					</span>
				</button>
				<button
					@click="$emit('updateStatus', 'hidden')"
					v-tooltip.top="setTooltipData('Hide')"
					class="mr-1 bg-gray-700 text-white rounded py-2 mt-2 hover:bg-gray-600">
					<span class="relative">
						<span class="material-icons" v-if="!isLoadingActions.includes('hidden')">visibility_off</span>
						<loader v-if="isLoadingActions.includes('hidden')" is-mini />
					</span>
				</button>
				<button
					v-if="status === 'hidden' || status === 'done'"
					v-tooltip.top="setTooltipData('Delete')"
					@click="$emit('remove')"
					class="bg-red-700 text-white rounded py-2 mt-2 hover:bg-red-600">
					<span class="relative">
						<span class="material-icons" v-if="!isLoadingActions.includes('delete')">delete</span>
						<loader v-if="isLoadingActions.includes('delete')" is-mini />
					</span>
				</button>
				<button
					@click="$emit('export')"
					v-tooltip.top="setTooltipData('Export to CSV')"
					class="bg-blue-700 text-white rounded py-2 mt-2 hover:bg-blue-600">
					<span class="relative">
						<span class="material-icons" v-if="!isLoadingActions.includes('csv')">description</span>
						<loader v-if="isLoadingActions.includes('csv')" is-mini />
					</span>
				</button>
				<button
					@click="$emit('export', 'jpg')"
					v-tooltip.top="setTooltipData('Export to JPEG')"
					class="bg-blue-700 ml-1 text-white rounded py-2 mt-2 hover:bg-blue-600">
					<span class="relative">
						<span class="material-icons" v-if="!isLoadingActions.includes('jpg')">image</span>
						<loader v-if="isLoadingActions.includes('jpg')" is-mini />
					</span>
				</button>
				<button
					@click="$emit('export', 'xlsx')"
					v-tooltip.top="setTooltipData('Export to XLSX')"
					class="bg-blue-700 text-white ml-1 rounded py-2 mt-2 hover:bg-blue-600">
					<span class="relative">
						<span class="material-icons" v-if="!isLoadingActions.includes('xlsx')">list</span>
						<loader v-if="isLoadingActions.includes('xlsx')" is-mini />
					</span>
				</button>
			</div>
		</vue-draggable-resizable>
	</div>
</template>

<script>
	import SetTooltipData from "src/mixins/SetTooltipData";
	import VueDraggableResizable from "src/components/UIElements/VueDraggableResizable/VueDraggableResizable";

	export default {
		name: 'TasksMultipleActionsModal',
		emits: [
			'export',
			'remove',
			'updateStatus'
		],
		mixins: [
			SetTooltipData
		],
		components: {
			VueDraggableResizable
		},
		props: {
			status: {
				required: false,
				type: String,
				default: null
			},
			isLoadingActions: {
				type: Object,
				required: true
			}
		}
	}
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
