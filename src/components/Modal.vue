<template>
	<div
		class="fixed inset-0 z-50 flex bg-black/50"
		data-name="overlay"
		@click="close"
	>
		<div
			class="m-auto max-h-[95%] max-w-[95%] rounded-lg bg-white dark:bg-gray-900"
			:class="modalClass"
		>
			<slot name="modal-body"></slot>
		</div>
	</div>
</template>

<script lang="ts" setup>
	import { onUnmounted, ref } from 'vue';

	interface Props {
		modalClass: string;
		closeOnBgClick: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close']);
	const initialUrl = ref(location.href);

	onUnmounted(() => {
		if (location.href !== initialUrl.value) {
			history.pushState({}, '', initialUrl.value);
		}
	});

	function close(e: MouseEvent) {
		if (props.closeOnBgClick) {
			const target = e.target as HTMLDivElement;

			if (target.dataset.name === 'overlay') {
				emit('close');
			}
		}
	}
</script>
