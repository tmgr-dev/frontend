import { Ref, ref } from 'vue';

export const dialogState = (): [Ref<boolean>, () => void] => {
	const isOpen: Ref<boolean> = ref(false);

	function closeDialog(): void {
		isOpen.value = false;
	}

	return [isOpen, closeDialog];
};
