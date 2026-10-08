import { onBeforeUnmount, onMounted, ref } from 'vue';

export function useDocumentTheme() {
	const dark = ref(false);
	const version = ref(0);
	let observer: MutationObserver | null = null;

	const read = () => {
		dark.value = document.documentElement.classList.contains('dark');
		version.value++;
	};

	onMounted(() => {
		read();
		observer = new MutationObserver(read);
		observer.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ['class'],
		});
	});
	onBeforeUnmount(() => observer?.disconnect());

	return { dark, version };
}
