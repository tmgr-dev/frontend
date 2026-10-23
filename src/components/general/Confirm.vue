<template>
	<Modal
		:close-on-bg-click="cancelOnBgClick"
		modal-class="p-6"
		@close="$emit('onCancel')"
	>
		<template #modal-body>
			<slot name="title">
				<div class="text-bold text-xl text-ink">
					<slot name="title-content">
						{{ title }}
					</slot>
				</div>
			</slot>

			<slot name="body">
				<p class="mt-1 text-ink-muted">
					<slot name="body-content">
						{{ body }}
					</slot>
				</p>
			</slot>

			<slot name="footer">
				<div class="mt-6 flex justify-end gap-3">
					<Button type="button" variant="outline" @click="$emit('onCancel')">
						Cancel
					</Button>
					<Button type="button" @click="$emit('onOk')"> Ok </Button>
				</div>
			</slot>
		</template>
	</Modal>
</template>

<script lang="ts">
	import Modal from '@/components/Modal.vue';
	import { Button } from '@/components/ui/button';

	interface Props {
		title: String;
		body: String;
		cancelOnBgClick?: Boolean;
	}

	interface Data extends Props {}

	export default {
		name: 'Confirm',
		components: { Modal, Button },
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
