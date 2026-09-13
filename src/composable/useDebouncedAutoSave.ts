import { getCurrentScope, onScopeDispose, ref, type Ref, watch } from 'vue';

interface Props<T> {
	formRef: Ref<T>;
	onSave: (snapshot: T) => Promise<void> | void;
	fieldsToWatch: (keyof T)[];
	delay?: number;
	suppressDebounceForOnce?: Ref<boolean>;
	onError?: (error: unknown, snapshot: T) => void;
	makeSnapshot?: (value: T) => T;
	onDirty?: (snapshot: T) => void;
	enabled?: () => boolean;
}

/** One serial writer; queued payloads never read a later form's identity. */
export function useDebouncedAutoSave<T>({
	formRef,
	onSave,
	fieldsToWatch,
	delay = 2000,
	suppressDebounceForOnce,
	onError,
	makeSnapshot,
	onDirty,
	enabled,
}: Props<T>): [
	Ref<boolean>,
	() => void,
	(force?: boolean) => Promise<void>,
	(snapshot: T) => Promise<void>,
] {
	const isSaving = ref(false);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let pending: T | undefined;
	let running: Promise<void> | undefined;
	let disposed = false;
	const snapshot = () =>
		JSON.parse(
			JSON.stringify(
				makeSnapshot ? makeSnapshot(formRef.value) : formRef.value,
			),
		) as T;
	const cancel = () => {
		clearTimeout(timer);
		timer = undefined;
		pending = undefined;
	};
	const drain = (): Promise<void> => {
		clearTimeout(timer);
		timer = undefined;
		if (running)
			return running.then(() => (pending === undefined ? undefined : drain()));
		if (pending === undefined) return Promise.resolve();
		const sent = pending;
		pending = undefined;
		isSaving.value = true;
		running = Promise.resolve()
			.then(() => onSave(sent))
			.catch((error) => {
				if (onError) onError(error, sent);
				else console.error('Autosave failed', error);
			})
			.finally(() => {
				running = undefined;
				isSaving.value = false;
			});
		return running.then(() => {
			if (pending !== undefined) {
				if (disposed) return drain();
				timer = setTimeout(() => {
					void drain();
				}, delay);
			}
		});
	};
	const flush = (force = false) => {
		if (force && !disposed) pending = snapshot();
		return drain();
	};
	watch(
		() => fieldsToWatch.map((key) => formRef.value[key]),
		() => {
			if (disposed) return;
			if (suppressDebounceForOnce?.value) {
				suppressDebounceForOnce.value = false;
				return;
			}
			if (enabled && !enabled()) return;
			pending = snapshot();
			onDirty?.(pending);
			clearTimeout(timer);
			if (!running)
				timer = setTimeout(() => {
					void drain();
				}, delay);
		},
		{ deep: true, flush: 'sync' },
	);
	if (getCurrentScope())
		onScopeDispose(() => {
			disposed = true;
			void drain();
		});
	return [
		isSaving,
		cancel,
		flush,
		(value: T) => {
			pending = JSON.parse(JSON.stringify(value));
			onDirty?.(pending as T);
			return drain();
		},
	];
}
