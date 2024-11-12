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
				<div class="float-right mt-3 text-tmgr-blue dark:text-gray-300">
					<new-button
						class="pr-2"
						button-class="bg-green-700 hover:bg-green-600"
						@click="$emit('onOk')"
					>
						Ok
					</new-button>

					<new-button color="gray" @click="$emit('onCancel')">
						Cancel
					</new-button>
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
