import { ref, onMounted, onBeforeUnmount, type Ref } from 'vue';

export interface RoutineViewport {
	width: Ref<number>;
	isMobile: Ref<boolean>;
	isTablet: Ref<boolean>;
	bind: (el: HTMLElement | null) => void;
}

export function useDailyRoutineViewport(): RoutineViewport {
	const width = ref<number>(typeof window !== 'undefined' ? window.innerWidth : 1280);
	const isMobile = ref<boolean>(false);
	const isTablet = ref<boolean>(false);
	let observer: ResizeObserver | null = null;
	let target: HTMLElement | null = null;

	function compute(w: number) {
		width.value = w;
		isMobile.value = w < 640;
		isTablet.value = w < 960;
	}

	function bind(el: HTMLElement | null) {
		target = el;
		if (!el) return;
		if (typeof ResizeObserver !== 'undefined') {
			observer = new ResizeObserver(entries => {
				for (const entry of entries) {
					const cw = entry.contentRect.width;
					if (cw > 0) compute(cw);
				}
			});
			observer.observe(el);
		}
		compute(el.getBoundingClientRect().width || width.value);
	}

	function onResize() {
		if (!target) compute(window.innerWidth);
	}

	onMounted(() => {
		compute(typeof window !== 'undefined' ? window.innerWidth : 1280);
		window.addEventListener('resize', onResize);
	});

	onBeforeUnmount(() => {
		window.removeEventListener('resize', onResize);
		observer?.disconnect();
	});

	return { width, isMobile, isTablet, bind };
}
