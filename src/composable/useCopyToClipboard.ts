// composables/useCopyToClipboard.ts
import { ref, type Ref } from 'vue';

type CopyFn = (value: string) => Promise<boolean>;

export const useCopyToClipboard = (): [
	CopyFn,
	Ref<string | null>,
	(value: string | null) => void,
] => {
	const copiedText = ref<string | null>(null);

	const copy: CopyFn = async (value: string) => {
		if (!navigator?.clipboard) {
			console.warn('Clipboard not supported');
			return false;
		}

		try {
			await navigator.clipboard.writeText(value);
			copiedText.value = value;
			return true;
		} catch (error) {
			console.error('Copy failed', error);
			copiedText.value = null;
			return false;
		}
	};

	const setCopiedText = (value: string | null) => {
		copiedText.value = value;
	};

	return [copy, copiedText, setCopiedText];
};
