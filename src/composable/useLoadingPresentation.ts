import {
	computed,
	onScopeDispose,
	ref,
	toValue,
	watch,
	type MaybeRefOrGetter,
} from 'vue';

/** Presentation only: request owners control pending, success and context identity. */
export function useLoadingPresentation(options: {
	pending: MaybeRefOrGetter<boolean>;
	loaded?: MaybeRefOrGetter<boolean>;
	hasData?: MaybeRefOrGetter<boolean>;
	error?: MaybeRefOrGetter<unknown>;
	delay?: number;
}) {
	const showIndicator = ref(false);
	let timer: ReturnType<typeof setTimeout> | undefined;
	const clear = () => {
		clearTimeout(timer);
		timer = undefined;
		showIndicator.value = false;
	};
	const available = computed(
		() => !!toValue(options.loaded) || !!toValue(options.hasData),
	);
	const busy = computed(() => !!toValue(options.pending));
	const showSkeleton = computed(
		() => !available.value && !toValue(options.error),
	);
	watch(
		busy,
		(pending) => {
			clear();
			if (pending)
				timer = setTimeout(() => {
					showIndicator.value = true;
					timer = undefined;
				}, options.delay ?? 120);
		},
		{ immediate: true, flush: 'sync' },
	);
	onScopeDispose(clear);
	return { available, busy, showSkeleton, showIndicator };
}
