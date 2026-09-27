import { onBeforeUnmount, ref, type Ref } from 'vue';

const TICK_MS = 30_000;
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;
let subscribers = 0;

/** One shared 30 s clock for every timeAgo/dueTime node, instead of a timer per node. */
export const usePluginTicker = (): Ref<number> => {
	subscribers++;
	if (!timer) timer = setInterval(() => (now.value = Date.now()), TICK_MS);
	onBeforeUnmount(() => {
		if (--subscribers <= 0 && timer) {
			clearInterval(timer);
			timer = null;
		}
	});
	return now;
};
