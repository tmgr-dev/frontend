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
	import { onMounted, onUnmounted, ref } from 'vue';

	interface Props {
		modalClass: string;
		closeOnBgClick?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close']);
	const initialUrl = ref(location.href);

	onMounted(() => {
		document.addEventListener('keydown', closeByEscape);
	});

	onUnmounted(() => {
		if (location.href !== initialUrl.value) {
			history.pushState({}, '', initialUrl.value);
		}
		document.removeEventListener('keydown', closeByEscape);
	});

	function closeByEscape(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			const secondModal = document.querySelector('#modal2');
			if (secondModal) {
				secondModal.remove();
			} else {
				emit('close');
			}
		}
	}

	function close(e: MouseEvent) {
		if (props.closeOnBgClick) {
			const target = e.target as HTMLDivElement;

			if (target.dataset.name === 'overlay') {
				emit('close');
			}
		}
	}
</script>
