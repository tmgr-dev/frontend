import { ref, Ref, watch } from 'vue';

interface Props<T> {
	formRef: Ref<T>;
	onSave: () => Promise<void> | void;
	fieldsToWatch: (keyof T)[];
	delay?: number;
	suppressDebounceForOnce?: Ref<boolean>;
}

// Define the return type clearly
type DebouncedAutoSaveReturn = [
	isSaving: Ref<boolean>,
	cancelPendingAutoSave: () => void
];

export function useDebouncedAutoSave<T>({
	formRef,
	onSave,
	fieldsToWatch,
	delay = 2000,
	suppressDebounceForOnce,
}: Props<T>): DebouncedAutoSaveReturn {
	let saveTimeout: ReturnType<typeof setTimeout> | null = null;
	const isSaving = ref(false);
	
	// Function to cancel any pending auto-saves
	const cancelPendingAutoSave = () => {
		if (saveTimeout) {
			clearTimeout(saveTimeout);
			saveTimeout = null;
		}
	};

	watch(
		fieldsToWatch
			? () =>
					fieldsToWatch.reduce((result, key) => {
						result[key] = formRef.value[key];
						return result;
					}, {} as Partial<T>)
			: formRef,
		async () => {
			// Check if we should suppress this auto-save
			if (suppressDebounceForOnce?.value) {
				suppressDebounceForOnce.value = false;
				cancelPendingAutoSave(); // Cancel any pending auto-saves
				return;
			}

			// Don't schedule new saves if already saving
			if (isSaving.value) return;

			// Cancel any previous pending saves
			cancelPendingAutoSave();

			// Schedule a new save
			saveTimeout = setTimeout(async () => {
				// Check again before saving in case suppress flag was set after timeout was scheduled
				if (suppressDebounceForOnce?.value) {
					suppressDebounceForOnce.value = false;
					saveTimeout = null;
					return;
				}
				
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

	// Return both the saving state and a function to cancel pending saves
	return [isSaving, cancelPendingAutoSave];
}
