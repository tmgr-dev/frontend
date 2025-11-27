<template>
	<Modal
		:close-on-bg-click="cancelOnBgClick"
		modal-class="p-6"
		@close="$emit('onCancel')"
	>
		<template #modal-body>
			<slot name="title">
				<div class="text-bold text-xl text-tmgr-blue dark:text-gray-300">
					<slot name="title-content">
						{{ title }}
					</slot>
				</div>
			</slot>

			<slot name="body">
				<p class="mt-1 text-tmgr-blue dark:text-gray-300">
					<slot name="body-content">
						{{ body }}
					</slot>
				</p>
			</slot>

			<slot name="footer">
				<div class="mt-6 flex justify-end gap-3">
					<button
						type="button"
						class="rounded-md bg-gray-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-500 focus:ring-offset-2"
						@click="$emit('onCancel')"
					>
						Cancel
					</button>
					<button
						type="button"
						class="rounded-md bg-green-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2"
						@click="$emit('onOk')"
					>
						Ok
					</button>
				</div>
			</slot>
		</template>
	</Modal>
</template>

<script lang="ts">
	import Modal from '@/components/Modal.vue';

	interface Props {
		title: String;
		body: String;
		cancelOnBgClick?: Boolean;
	}

	interface Context {
		attrs: Props;
		emit: Function;
	}

	interface Data extends Props {}

	export default {
		name: 'Confirm',
		components: { Modal },
		emits: ['onOk', 'onCancel'],
		props: {
			title: String,
			body: String,
			cancelOnBgClick: {
				type: Boolean,
				default: true,
			},
		},
		setup({ title, body, cancelOnBgClick = true }: Props): Data {
			return {
				cancelOnBgClick,
				title,
				body,
			};
		},
	};
</script>

<style scoped></style>
