<template>
	<div class="draggable-block">
		<vue-draggable-resizable :resizable="false" :w="250" class-name="rounded bg-green-600 py-5 px-2">
			<slot></slot>
			<div class="w-full text-center">
				<a href="#close"
					 class="absolute top-0 right-0 pr-1 pt-1 text-gray-400 hover:text-gray-100"
					 @click.prevent="$emit('close')">
					<span class="material-icons text-bold">close</span>
				</a>
				<button
					@click="$emit('updateStatus', 'done')"
					class="mr-1 w-1/5 bg-green-700 text-white rounded py-2 mt-2 hover:bg-green-600">
					<span class="relative">
						<span class="material-icons text-bold">done</span>
					</span>
				</button>
				<button
					@click="$emit('updateStatus', 'active')"
					class="mr-1 w-1/5 bg-purple-700 text-white rounded py-2 mt-2 hover:bg-purple-600">
					<span class="relative">
						<span class="material-icons text-bold">refresh</span>
					</span>
				</button>
				<button
					@click="$emit('updateStatus', 'hidden')"
					class="mr-1 w-1/5 bg-green-700 text-white rounded py-2 mt-2 hover:bg-green-600">
				<span class="relative">
					<span class="material-icons">visibility_off</span>
				</span>
				</button>
				<button
					v-if="status === 'hidden' || status === 'done'"
					@click="$emit('remove')"
					class="w-1/5 bg-red-700 text-white rounded py-2 mt-2 hover:bg-red-600">
				<span class="relative">
					<span class="material-icons">delete</span>
				</span>
				</button>
				<button
					@click="$emit('export')"
					class="w-1/5 bg-blue-700 text-white rounded py-2 mt-2 hover:bg-blue-600">
				<span class="relative">
					<span class="material-icons">description</span>
				</span>
				</button>
				<button
					@click="$emit('export', 'jpg')"
					class="w-1/5 bg-blue-700 ml-1 text-white rounded py-2 mt-2 hover:bg-blue-600">
				<span class="relative">
					<span class="material-icons">image</span>
				</span>
				</button>
				<button
					@click="$emit('export', 'xlsx')"
					class="w-1/5 bg-blue-700 text-white ml-1 rounded py-2 mt-2 hover:bg-blue-600">
				<span class="relative">
					<span class="material-icons">list</span>
				</span>
				</button>
			</div>
		</vue-draggable-resizable>
	</div>
</template>

<script>
	import VueDraggableResizable from "src/components/UIElements/VueDraggableResizable/VueDraggableResizable";

	export default {
		name: 'TasksMultipleActionsModal',
		emits: [
			'export',
			'remove',
			'updateStatus'
		],
		components: {
			VueDraggableResizable
		},
		props: {
			status: {
				required: false,
				type: String,
				default: null
			}
		}
	}
</script>
