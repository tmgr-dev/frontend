<template>
	<div class="fixed inset-0 z-50 flex" data-name="overlay">
		<div class="absolute inset-0 bg-black/50" @click="zooming" />

		<div
			class="relative m-auto max-w-full bg-white dark:bg-gray-900 md:rounded-[8px]"
			:class="[modalClass, { 'zoom-effect': isZooming }]"
		>
			<slot name="modal-body"></slot>
		</div>
	</div>
</template>

<script lang="ts" setup>
	import { onMounted, onUnmounted, ref } from 'vue';
	import { useStore } from 'vuex';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';

	interface Props {
		modalClass?: string;
		closeOnBgClick?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close', 'closingModal']);
	const initialUrl = ref(location.href);
	const store = useStore();
	const modalId = ref(`modal-${Date.now()}-${Math.random()}`);
	const isClosing = ref(false);
	const { registerModal, unregisterModal } = useModalEscHandler();

	onMounted(() => {
		document.body.classList.add('overflow-hidden');
		store.commit('pushModalToStack', modalId.value);

		registerModal(modalId.value, ({ hasUnderlying }) => {
			if (isClosing.value) return;
			isClosing.value = true;

			if (hasUnderlying) {
				emit('closingModal');
			} else {
				emit('close');
			}
		});
	});

	onUnmounted(() => {
		unregisterModal(modalId.value);
		store.commit('removeModalFromStack', modalId.value);

		if (store.state.modalStack.length === 0) {
			document.body.classList.remove('overflow-hidden');
		}

		if (location.href !== initialUrl.value) {
			history.replaceState({}, '', initialUrl.value);
		}
	});

	const isZooming = ref();
	function zooming() {
		isZooming.value = true;
		setTimeout(() => {
			isZooming.value = false;
		}, 300);
	}
</script>

<style scoped>
	@keyframes zoomInOut {
		0% {
			transform: scale(1);
		}
		50% {
			transform: scale(1.05);
		}
		100% {
			transform: scale(1);
		}
	}

	.zoom-effect {
		animation: zoomInOut 0.3s ease-in-out;
	}
</style>
