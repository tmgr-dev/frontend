<template>
	<div class="fixed inset-0 z-50 flex" data-name="overlay" @click="close">
		<div ref="$modalOverlay" class="absolute inset-0 bg-black/50" />

		<div
			ref="$modal"
			class="m-auto max-h-[95%] max-w-[95%] rounded-lg bg-white dark:bg-gray-900"
			:class="modalClass"
		>
			<slot />
		</div>
	</div>
</template>

<script setup lang="ts">
	import gsap from 'gsap';
	import { onMounted, ref, Ref } from 'vue';

	interface Props {
		closeOnBgClick?: boolean;
		modalClass?: string;
		event: Event;
	}

	const DURATION = 1;
	const emit = defineEmits(['close', 'show']);
	const props = withDefaults(defineProps<Props>(), {
		closeOnBgClick: false,
		modalClass: '',
	});

	const $modalOverlay: Ref<HTMLDivElement | null> = ref(null);
	const $modal: Ref<HTMLDivElement | null> = ref(null);

	onMounted(() => {
		gsap.set([$modalOverlay], { autoAlpha: 0 });
		showModal();
	});

	function showModal() {
		const newRect = getPosition(
			$modal.value,
			props.event.target as HTMLElement,
		);

		gsap.set($modal.value, {
			x: newRect.left,
			y: newRect.top,
			width: newRect.width,
			height: newRect.height,
		});

		gsap.to($modalOverlay.value, { autoAlpha: 1, duration: 1 });
		gsap.to($modal.value, {
			x: 0,
			y: 0,
			width: window.innerWidth - window.innerWidth * 0.05,
			height: window.innerHeight - window.innerHeight * 0.05,
			duration: DURATION,
		});
	}

	function close(e: MouseEvent) {
		if ((e.target as HTMLDivElement).classList.contains('overlay')) {
			//forceClose();
			console.log('cho');
			gsap.to($modal.value, {
				x: 0,
				y: 0,
				width: 0,
				height: 0,
				duration: DURATION,
			});
		}
	}

	function forceClose() {
		gsap.set([$modalOverlay.value, $modal.value], { autoAlpha: 0 });
	}

	function getPosition(elem: HTMLElement | null, target: HTMLElement | null) {
		const targetRect = target!.getBoundingClientRect();
		const elemRect = elem!.getBoundingClientRect();

		gsap.set(elem, {
			x: 0,
			y: 0,
			width: targetRect.width,
			height: targetRect.height,
		});

		const newRect = elem!.getBoundingClientRect();
		gsap.set(elem, { width: elemRect.width, height: elemRect.height });

		return {
			left: targetRect.left - newRect.left,
			top: targetRect.top - newRect.top,
			width: newRect.width,
			height: newRect.height,
		};
	}
</script>
