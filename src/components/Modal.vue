<template>
	<Teleport to="body">
		<div class="fixed inset-0 z-50 flex" data-name="overlay" v-bind="$attrs">
			<div class="absolute inset-0 bg-black/50" @click="onBackdropClick" />

			<div
				ref="dialogEl"
				role="dialog"
				aria-modal="true"
				class="relative m-auto max-w-full border border-line bg-surface shadow-lg md:rounded-[8px]"
				:class="[modalClass, { 'zoom-effect': isZooming }]"
			>
				<slot name="modal-body"></slot>
			</div>
		</div>
	</Teleport>
</template>

<script lang="ts" setup>
	import { useDialogFocus } from '@/composable/useDialogFocus';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';
	import {
		getCurrentInstance,
		onBeforeUnmount,
		onMounted,
		onUnmounted,
		ref,
	} from 'vue';
	import { useStore } from 'vuex';

	interface Props {
		modalClass?: string;
		closeOnBgClick?: boolean;
	}

	defineOptions({ inheritAttrs: false });
	const props = defineProps<Props>();
	const emit = defineEmits(['close', 'closingModal']);
	const initialUrl = ref(location.href);
	const store = useStore();
	const modalId = ref(`modal-${Date.now()}-${Math.random()}`);
	const isClosing = ref(false);
	const { registerModal, unregisterModal, hasUnderlyingModal } =
		useModalEscHandler();
	const hasClosingModalListener =
		!!getCurrentInstance()?.vnode.props?.onClosingModal;
	const dialogEl = ref<HTMLElement | null>(null);
	const dialogFocus = useDialogFocus(modalId.value, dialogEl, 'Dialog');
	dialogFocus.remember();

	function requestClose({ hasUnderlying }: { hasUnderlying: boolean }) {
		if (isClosing.value) return;
		isClosing.value = true;
		setTimeout(() => {
			isClosing.value = false;
		}, 300);

		if (hasUnderlying && hasClosingModalListener) {
			emit('closingModal');
		} else {
			emit('close');
		}
	}

	onMounted(() => {
		document.body.classList.add('overflow-hidden');
		store.commit('pushModalToStack', modalId.value);

		registerModal(modalId.value, requestClose);
		dialogFocus.activate();
	});

	onBeforeUnmount(() => {
		dialogFocus.deactivate();
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

	function onBackdropClick() {
		if (!props.closeOnBgClick) {
			zooming();
			return;
		}
		const stack = store.state.modalStack;
		if (stack[stack.length - 1] !== modalId.value) return;
		requestClose({ hasUnderlying: hasUnderlyingModal() });
	}

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
