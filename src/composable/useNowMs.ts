import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue';

const clocks = new Map<
	number,
	{
		now: Ref<number>;
		count: number;
		timer?: ReturnType<typeof setInterval>;
		visibility: () => void;
	}
>();

/** Shared per-frequency wall clock. Hidden tabs catch up immediately on return. */
export function useNowMs(interval = 10_000): Ref<number> {
	let clock = clocks.get(interval);
	if (!clock) {
		clock = { now: ref(Date.now()), count: 0, visibility: () => {} };
		const entry = clock;
		entry.visibility = () => {
			clearInterval(entry.timer);
			entry.timer = undefined;
			if (entry.count && !document.hidden) {
				entry.now.value = Date.now();
				entry.timer = setInterval(() => {
					entry.now.value = Date.now();
				}, interval);
			}
		};
		clocks.set(interval, entry);
	}
	const entry = clock;
	let mounted = false;
	onMounted(() => {
		mounted = true;
		if (++entry.count === 1) {
			document.addEventListener('visibilitychange', entry.visibility);
			entry.visibility();
		}
	});
	onBeforeUnmount(() => {
		if (!mounted) return;
		if (--entry.count === 0) {
			clearInterval(entry.timer);
			entry.timer = undefined;
			document.removeEventListener('visibilitychange', entry.visibility);
		}
	});
	return entry.now;
}
