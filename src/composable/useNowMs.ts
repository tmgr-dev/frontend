import { ref, onMounted, onBeforeUnmount, type Ref } from 'vue';

// One shared ticker for every subscriber (routine rows can number in the
// dozens — a per-row setInterval would pile up timers for no benefit).
const nowMs = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;
let subscribers = 0;

/** Reactive "current time" updated every 10s while at least one component uses it. */
export function useNowMs(): Ref<number> {
	onMounted(() => {
		subscribers += 1;
		if (!timer) {
			nowMs.value = Date.now();
			timer = setInterval(() => {
				nowMs.value = Date.now();
			}, 10_000);
		}
	});
	onBeforeUnmount(() => {
		subscribers -= 1;
		if (subscribers <= 0 && timer) {
			clearInterval(timer);
			timer = null;
		}
	});
	return nowMs;
}
