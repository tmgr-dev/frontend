<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent
			class="max-w-[96vw] gap-0 border-none bg-black/95 p-0 sm:max-w-[92vw]"
		>
			<DialogTitle class="sr-only">{{ current?.name ?? 'Image' }}</DialogTitle>
			<DialogDescription class="sr-only">
				Image {{ index + 1 }} of {{ images.length }}
			</DialogDescription>

			<div
				class="flex items-center gap-3 px-4 py-3 pr-14 text-sm text-gray-200"
			>
				<span class="min-w-0 flex-1 truncate">{{ current?.name }}</span>
				<span v-if="current?.size" class="flex-shrink-0 text-gray-400">
					{{ formatFileSize(current.size) }}
				</span>
				<span v-if="images.length > 1" class="flex-shrink-0 text-gray-400">
					{{ index + 1 }} / {{ images.length }}
				</span>
				<button
					type="button"
					class="flex-shrink-0 rounded p-1.5 text-gray-300 transition-colors hover:bg-white/10 hover:text-white"
					title="Download"
					@click="download"
				>
					<Download :size="18" />
				</button>
			</div>

			<div
				class="relative flex h-[78vh] items-center justify-center overflow-hidden"
			>
				<button
					v-if="images.length > 1"
					type="button"
					class="absolute left-2 z-10 rounded-full bg-black/50 p-2 text-white transition-colors hover:bg-black/70"
					title="Previous"
					@click="step(-1)"
				>
					<ChevronLeft :size="24" />
				</button>

				<img
					v-if="currentUrl"
					:src="currentUrl"
					:alt="current?.name"
					class="max-h-full max-w-full object-contain"
				/>
				<Loader2 v-else :size="28" class="animate-spin text-gray-400" />

				<button
					v-if="images.length > 1"
					type="button"
					class="absolute right-2 z-10 rounded-full bg-black/50 p-2 text-white transition-colors hover:bg-black/70"
					title="Next"
					@click="step(1)"
				>
					<ChevronRight :size="24" />
				</button>
			</div>
		</DialogContent>
	</Dialog>
</template>

<script setup lang="ts">
	import { computed, onBeforeUnmount, ref, watch } from 'vue';
	import { ChevronLeft, ChevronRight, Download, Loader2 } from 'lucide-vue-next';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { fetchFileObjectUrl } from '@/actions/tmgr/files';
	import { formatFileSize } from '@/utils/attachments';
	import { type GalleryImage, stepIndex } from '@/utils/galleryNavigation';

	const props = defineProps<{
		/** Images in the order the list shows them: a task's attachments, or a whole workspace's. */
		images: GalleryImage[];
		/** The image to open on; null keeps the gallery closed. */
		startId: number | null;
		/** Blob URLs the list already fetched, reused so opening costs no extra request. */
		urls: Record<number, string>;
	}>();

	const emit = defineEmits<{ (event: 'close'): void }>();

	const index = ref(0);
	/** URLs this component fetched itself, and therefore has to revoke. */
	const ownUrls = ref<Record<number, string>>({});

	const open = computed(() => props.startId !== null);
	const current = computed<GalleryImage | undefined>(() => props.images[index.value]);
	const currentUrl = computed(() => {
		const id = current.value?.id;
		return id ? (props.urls[id] ?? ownUrls.value[id] ?? null) : null;
	});

	const ensureUrl = async (file?: GalleryImage) => {
		if (!file || props.urls[file.id] || ownUrls.value[file.id]) {
			return;
		}
		try {
			ownUrls.value[file.id] = await fetchFileObjectUrl(file.id);
		} catch {
			// Leaves the spinner in place; the file is still downloadable from the list.
		}
	};

	const step = (delta: number) => {
		index.value = stepIndex(index.value, props.images.length, delta);
		ensureUrl(current.value);
	};

	const onKeydown = (event: KeyboardEvent) => {
		if (event.key === 'ArrowLeft') {
			step(-1);
		} else if (event.key === 'ArrowRight') {
			step(1);
		}
	};

	const onOpenChange = (value: boolean) => {
		if (!value) {
			emit('close');
		}
	};

	const download = () => {
		const url = currentUrl.value;
		if (!url || !current.value) {
			return;
		}
		const link = document.createElement('a');
		link.href = url;
		link.download = current.value.name;
		link.click();
	};

	const releaseOwnUrls = () => {
		Object.values(ownUrls.value).forEach((url) => URL.revokeObjectURL(url));
		ownUrls.value = {};
	};

	watch(
		() => props.startId,
		(startId) => {
			if (startId === null) {
				// Esc goes to the topmost radix dialog, so closing here never closes the task behind it.
				document.removeEventListener('keydown', onKeydown);
				releaseOwnUrls();
				return;
			}
			const found = props.images.findIndex((file) => file.id === startId);
			index.value = found === -1 ? 0 : found;
			ensureUrl(current.value);
			document.addEventListener('keydown', onKeydown);
		},
		{ immediate: true },
	);

	onBeforeUnmount(() => {
		document.removeEventListener('keydown', onKeydown);
		releaseOwnUrls();
	});
</script>
