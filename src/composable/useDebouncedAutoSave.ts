import { ref, Ref, watch } from 'vue';

interface Props<T> {
	formRef: Ref<T>;
	onSave: () => Promise<void> | void;
	fieldsToWatch: (keyof T)[];
	delay?: number;
	suppressDebounceForOnce?: Ref<boolean>;
	isEnabled?: Ref<boolean>;  // Add this parameter
}

export function useDebouncedAutoSave<T>({
																					formRef,
																					onSave,
																					fieldsToWatch,
																					delay,
																					suppressDebounceForOnce,
																					isEnabled = ref(true),  // Default to enabled
																				}: Props<T>) {
	let saveTimeout: ReturnType<typeof setTimeout> | null = null;
	let isSaving = ref(false);

	watch(
		fieldsToWatch
			? () =>
				fieldsToWatch.reduce((result, key) => {
					result[key] = formRef.value[key];
					return result;
				}, {} as Partial<T>)
			: formRef,
		async () => {
			// Add check for enabled state
			if (!isEnabled.value) return;

			if (suppressDebounceForOnce?.value) {
				suppressDebounceForOnce.value = false;
				return;
			}

			if (isSaving.value) return;

			if (saveTimeout) {
				clearTimeout(saveTimeout);
			}

			saveTimeout = setTimeout(async () => {
				isSaving.value = true;
				try {
					await onSave();
				} catch (e) {
					console.error('debounce error', e);
				} finally {
					isSaving.value = false;
					saveTimeout = null;
				}
			}, delay);
		},
		{ deep: true },
	);

	return [isSaving];
}
