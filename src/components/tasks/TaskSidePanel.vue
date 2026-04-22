<template>
	<div class="fixed inset-0 z-50 font-display" data-name="task-side-panel">
		<div
			class="absolute inset-0 animate-tmgr-fade-in bg-black/35 backdrop-blur-[2px]"
			@click="handleBackdrop"
		></div>

		<aside
			class="absolute inset-0 flex flex-col overflow-hidden border border-line bg-surface shadow-tmgr-lg animate-tmgr-slide-in-right
				sm:right-3 sm:top-3 sm:bottom-3 sm:left-auto sm:rounded-panel sm:w-[640px] sm:max-w-[calc(100vw-24px)]
				lg:w-[720px] xl:w-[760px]"
			role="dialog"
			aria-modal="true"
		>
			<slot></slot>
		</aside>
	</div>
</template>

<script lang="ts" setup>
	import { onMounted, onUnmounted, ref } from 'vue';
	import { useStore } from 'vuex';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';

	interface Props {
		closeOnBgClick?: boolean;
	}

	const props = withDefaults(defineProps<Props>(), {
		closeOnBgClick: true,
	});
	const emit = defineEmits(['close']);

	const store = useStore();
	const modalId = ref(`task-side-panel-${Date.now()}-${Math.random()}`);
	const isClosing = ref(false);
	const initialUrl = ref(location.href);
	const { registerModal, unregisterModal } = useModalEscHandler();

	function handleBackdrop() {
		if (props.closeOnBgClick) {
			triggerClose();
		}
	}

	function triggerClose() {
		if (isClosing.value) return;
		isClosing.value = true;
		emit('close');
	}

	onMounted(() => {
		document.body.classList.add('overflow-hidden');
		store.commit('pushModalToStack', modalId.value);
		registerModal(modalId.value, () => triggerClose());
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
</script>
