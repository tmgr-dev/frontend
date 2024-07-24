<template>
	<div
		id="backdrop"
		class="fixed inset-0 z-50 flex"
		data-name="overlay"
		@click="close"
	>
		<div class="absolute inset-0 bg-black/50" />

		<div
			class="relative m-auto max-w-full bg-white dark:bg-gray-900 md:rounded-lg"
			:class="modalClass"
		>
			<slot name="modal-body"></slot>
		</div>
	</div>
</template>

<script lang="ts" setup>
	import { onMounted, onUnmounted, ref } from 'vue';
	import { useStore } from 'vuex';

	interface Props {
		modalClass?: string;
		closeOnBgClick?: boolean;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['close', 'closingModal']);
	const initialUrl = ref(location.href);
	const store = useStore();

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
			if (store.state.openModals) {
				emit('closingModal');
			} else {
				emit('close');
			}
		}
	}

	function close(e: MouseEvent) {
		const secondModal = document.querySelector('#modal3');
		const thirdModal = document.querySelector('#modal4');
		const target = e.target as HTMLDivElement;
		if (thirdModal && target == thirdModal) {
			emit('closingModal');
			return;
		}
		if (target === secondModal) {
			emit('closingModal');
			return;
		} else {
			if (props.closeOnBgClick) {
				if (target.dataset.name === 'overlay') {
					emit('close');
				}
			}
		}
	}
</script>
