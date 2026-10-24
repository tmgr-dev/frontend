<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent
			class="left-0 top-0 flex h-[100dvh] w-screen max-w-none translate-x-0 translate-y-0 flex-col gap-0 rounded-none border-none bg-black/95 p-0 text-white/90 data-[state=closed]:slide-out-to-left-0 data-[state=closed]:slide-out-to-top-0 data-[state=open]:slide-in-from-left-0 data-[state=open]:slide-in-from-top-0 sm:max-w-none sm:rounded-none"
		>
			<DialogTitle class="sr-only">{{ current?.name ?? 'File' }}</DialogTitle>
			<DialogDescription class="sr-only">
				File {{ index + 1 }} of {{ images.length }}
			</DialogDescription>

			<div
				class="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-3 pr-14 text-sm text-white/90"
				data-testid="viewer-header"
			>
				<span class="min-w-0 flex-1 truncate">{{ current?.name }}</span>
				<span v-if="current?.size" class="flex-shrink-0 text-white/60">
					{{ formatFileSize(current.size) }}
				</span>
				<span v-if="images.length > 1" class="flex-shrink-0 text-white/60">
					{{ index + 1 }} / {{ images.length }}
				</span>
				<span
					v-if="downloadError"
					role="alert"
					class="flex-shrink-0 text-red-300"
					data-testid="viewer-download-error"
				>
					Download failed
				</span>
				<div
					v-if="kind === 'image'"
					class="flex flex-shrink-0 items-center gap-0.5"
				>
					<button
						type="button"
						class="rounded p-1.5 text-white/80 transition-colors hover:bg-white/10 hover:text-white disabled:opacity-40 disabled:hover:bg-transparent"
						title="Zoom out"
						aria-label="Zoom out"
						:disabled="zoom <= ZOOM_MIN"
						@click="setZoom(zoomOut(zoom))"
					>
						<ZoomOut :size="18" />
					</button>
					<button
						type="button"
						class="rounded p-1.5 text-white/80 transition-colors hover:bg-white/10 hover:text-white disabled:opacity-40 disabled:hover:bg-transparent"
						title="Zoom in"
						aria-label="Zoom in"
						:disabled="zoom >= ZOOM_MAX"
						@click="setZoom(zoomIn(zoom))"
					>
						<ZoomIn :size="18" />
					</button>
					<button
						type="button"
						class="rounded p-1.5 text-white/80 transition-colors hover:bg-white/10 hover:text-white"
						title="Fit to window (0)"
						aria-label="Fit to window"
						@click="fit"
					>
						<Maximize :size="18" />
					</button>
					<button
						type="button"
						class="rounded px-2 py-1 text-xs text-white/80 transition-colors hover:bg-white/10 hover:text-white"
						title="Actual size (100%)"
						aria-label="Actual size"
						@click="actualSize"
					>
						100%
					</button>
				</div>
				<button
					type="button"
					class="flex-shrink-0 rounded p-1.5 text-white/80 transition-colors hover:bg-white/10 hover:text-white"
					title="Download"
					aria-label="Download"
					:disabled="downloading"
					@click="download"
				>
					<Loader2 v-if="downloading" :size="18" class="animate-spin" />
					<Download v-else :size="18" />
				</button>
			</div>

			<div class="relative flex min-h-0 flex-1 items-center justify-center">
				<button
					v-if="images.length > 1"
					type="button"
					class="absolute left-2 z-10 rounded-full bg-black/50 p-2 text-white transition-colors hover:bg-black/70"
					title="Previous"
					aria-label="Previous file"
					@click="step(-1)"
				>
					<ChevronLeft :size="24" />
				</button>

				<template v-if="kind === 'image'">
					<div
						ref="stage"
						class="absolute inset-0 flex items-center justify-center overflow-hidden"
						:class="
							zoom > 1
								? ['touch-none', dragging ? 'cursor-grabbing' : 'cursor-grab']
								: ''
						"
						data-testid="viewer-stage"
						@wheel="onWheel"
						@pointerdown="onPointerDown"
						@pointermove="onPointerMove"
						@pointerup="endDrag"
						@pointercancel="endDrag"
					>
						<div
							class="relative flex h-full w-full items-center justify-center"
							:style="{
								transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
							}"
							data-testid="viewer-image-transform"
						>
							<img
								v-if="layers.showThumb && layers.thumb"
								:src="layers.thumb"
								:alt="current?.name"
								aria-hidden="true"
								draggable="false"
								class="absolute max-h-full max-w-full select-none object-contain blur-sm"
							/>
							<img
								v-if="layers.full"
								ref="fullImage"
								:src="layers.full"
								:alt="current?.name"
								draggable="false"
								class="max-h-full max-w-full select-none object-contain"
								:class="{ 'opacity-0': layers.showThumb }"
								@load="markLoaded(current?.id)"
								@error="imageError = true"
							/>
						</div>
					</div>
					<button
						v-if="imageError"
						role="alert"
						class="relative text-white underline"
						@click="retryImage"
					>
						Could not load image. Retry
					</button>
					<Loader2
						v-if="!imageError && !layers.thumb && !layers.full"
						:size="28"
						class="animate-spin text-white/60"
					/>
				</template>

				<div
					v-else
					class="flex h-full min-h-0 w-full items-stretch justify-center px-14 pb-4"
				>
					<Loader2
						v-if="contentState === 'loading' || contentState === 'idle'"
						:size="28"
						class="animate-spin self-center text-white/60"
					/>
					<div
						v-else-if="contentState === 'error'"
						role="alert"
						class="flex flex-col items-center justify-center gap-3 self-center text-center text-white/90"
						data-testid="viewer-error"
					>
						<AlertCircle :size="28" class="text-white/60" />
						<p>Could not load file</p>
						<button
							type="button"
							class="rounded-md border border-white/30 px-3 py-1.5 text-sm text-white transition-colors hover:bg-white/10"
							@click="loadContent"
						>
							Retry
						</button>
					</div>
					<div
						v-else-if="contentState === 'unsupported'"
						class="flex flex-col items-center justify-center gap-3 self-center text-center text-white/90"
						data-testid="viewer-unsupported"
					>
						<FileIcon :size="28" class="text-white/60" />
						<p>Preview is not available here</p>
						<button
							type="button"
							class="rounded-md border border-white/30 px-3 py-1.5 text-sm text-white transition-colors hover:bg-white/10 disabled:opacity-50"
							:disabled="downloading"
							@click="download"
						>
							Download
						</button>
					</div>
					<iframe
						v-else-if="kind === 'pdf' && pdfUrl"
						:src="pdfUrl"
						:title="current?.name"
						class="h-full w-full max-w-5xl rounded bg-white"
						data-testid="viewer-pdf"
					></iframe>
					<div
						v-else-if="kind === 'markdown'"
						class="h-full w-full max-w-3xl overflow-auto rounded-md bg-surface p-6 text-ink"
						tabindex="0"
						:aria-label="`Preview of ${current?.name}`"
						data-viewer-scroll
						data-testid="viewer-markdown"
					>
						<MarkdownText
							:content="textContent"
							:link-task-keys="false"
							untrusted
						/>
					</div>
					<pre
						v-else-if="kind === 'text'"
						class="h-full w-full max-w-4xl overflow-auto whitespace-pre-wrap break-words rounded-md bg-surface p-4 font-mono text-xs text-ink"
						tabindex="0"
						:aria-label="`Preview of ${current?.name}`"
						data-viewer-scroll
						data-testid="viewer-text"
						>{{ textContent }}</pre>
				</div>

				<button
					v-if="images.length > 1"
					type="button"
					class="absolute right-2 z-10 rounded-full bg-black/50 p-2 text-white transition-colors hover:bg-black/70"
					title="Next"
					aria-label="Next file"
					@click="step(1)"
				>
					<ChevronRight :size="24" />
				</button>
			</div>
		</DialogContent>
	</Dialog>
</template>

<script setup lang="ts">
	import {
		fetchFileBlob,
		fetchFileObjectUrl,
		fileDisplayUrl,
		invalidateFileDisplayUrl,
		releaseFileDisplayUrl,
	} from '@/actions/tmgr/files';
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		actualSizeZoom,
		clampPan,
		type Point,
		previewKind,
		withinTextPreviewLimit,
		ZOOM_MAX,
		ZOOM_MIN,
		zoomByWheel,
		zoomIn,
		zoomOut,
	} from '@/utils/attachmentViewer';
	import { formatFileSize } from '@/utils/attachments';
	import { galleryLayers } from '@/utils/galleryLayers';
	import { type GalleryImage, stepIndex } from '@/utils/galleryNavigation';
	import {
		AlertCircle,
		ChevronLeft,
		ChevronRight,
		Download,
		FileIcon,
		Loader2,
		Maximize,
		ZoomIn,
		ZoomOut,
	} from 'lucide-vue-next';
	import { computed, onBeforeUnmount, ref, watch } from 'vue';

	const props = withDefaults(
		defineProps<{
			/** Files in the order the list shows them: a task's attachments, a whole workspace's, or one message's. */
			images: GalleryImage[];
			/** The file to open on; null keeps the gallery closed. */
			startId: number | null;
			/** The list's thumbnails, drawn while the full image loads. */
			urls?: Record<number, string>;
		}>(),
		{ urls: () => ({}) },
	);

	const emit = defineEmits<{ (event: 'close'): void }>();

	const index = ref(0);
	/** URLs this component resolved itself, and therefore has to release. */
	const ownUrls = ref<Record<number, string>>({});
	/** Full images that have painted; until then the thumbnail stays on top of nothing. */
	const fullLoaded = ref<Record<number, boolean>>({});

	const open = computed(() => props.startId !== null);
	const current = computed<GalleryImage | undefined>(
		() => props.images[index.value],
	);
	const kind = computed(() =>
		current.value ? previewKind(current.value) : 'image',
	);
	// The list's URLs are thumbnails, too small to stretch across the screen on their own - but
	// good enough to draw at once underneath while the full image loads (TM-237).
	const layers = computed(() =>
		galleryLayers(
			current.value?.id,
			props.urls,
			ownUrls.value,
			fullLoaded.value,
		),
	);

	const markLoaded = (id?: number) => {
		if (id) {
			fullLoaded.value = { ...fullLoaded.value, [id]: true };
		}
	};

	let disposed = false;
	let imageRequest = 0;
	const imageError = ref(false);
	const ensureUrl = async (file?: GalleryImage) => {
		const request = ++imageRequest;
		imageError.value = false;
		if (!file || ownUrls.value[file.id]) {
			return;
		}
		try {
			const url = await fileDisplayUrl(file.id);
			if (disposed || request !== imageRequest || !open.value) {
				releaseFileDisplayUrl(url);
				return;
			}
			ownUrls.value[file.id] = url;
		} catch {
			if (request === imageRequest) imageError.value = true;
		}
	};

	const retryImage = () => {
		const id = current.value?.id;
		if (!id) return;
		if (ownUrls.value[id]) releaseFileDisplayUrl(ownUrls.value[id]);
		delete ownUrls.value[id];
		invalidateFileDisplayUrl(id);
		void ensureUrl(current.value);
	};

	const zoom = ref(1);
	const pan = ref<Point>({ x: 0, y: 0 });
	const dragging = ref(false);
	const stage = ref<HTMLElement | null>(null);
	const fullImage = ref<HTMLImageElement | null>(null);

	const boundedPan = (point: Point, atZoom: number): Point => {
		const view = stage.value;
		const image = fullImage.value;
		if (!view || !image) return { x: 0, y: 0 };
		return clampPan(
			point,
			atZoom,
			{ width: view.clientWidth, height: view.clientHeight },
			{ width: image.clientWidth, height: image.clientHeight },
		);
	};
	const setZoom = (value: number) => {
		zoom.value = value;
		pan.value = boundedPan(pan.value, value);
	};
	const fit = () => {
		zoom.value = 1;
		pan.value = { x: 0, y: 0 };
		drag = null;
		dragging.value = false;
	};
	const actualSize = () => {
		const image = fullImage.value;
		if (image) setZoom(actualSizeZoom(image.naturalWidth, image.clientWidth));
	};
	const onWheel = (event: WheelEvent) => {
		event.preventDefault();
		setZoom(zoomByWheel(zoom.value, event.deltaY, event.ctrlKey));
	};

	let drag: { id: number; x: number; y: number; from: Point } | null = null;
	const onPointerDown = (event: PointerEvent) => {
		if (zoom.value <= 1 || event.button !== 0 || !event.isPrimary) return;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		drag = {
			id: event.pointerId,
			x: event.clientX,
			y: event.clientY,
			from: { ...pan.value },
		};
		dragging.value = true;
	};
	const onPointerMove = (event: PointerEvent) => {
		if (!drag || event.pointerId !== drag.id) return;
		pan.value = boundedPan(
			{
				x: drag.from.x + event.clientX - drag.x,
				y: drag.from.y + event.clientY - drag.y,
			},
			zoom.value,
		);
	};
	const endDrag = (event: PointerEvent) => {
		if (drag && event.pointerId !== drag.id) return;
		drag = null;
		dragging.value = false;
	};

	const contentState = ref<
		'idle' | 'loading' | 'ready' | 'error' | 'unsupported'
	>('idle');
	const textContent = ref('');
	const pdfUrl = ref<string | null>(null);
	let contentRequest = 0;
	let contentAbort: AbortController | null = null;

	const releasePdf = () => {
		if (pdfUrl.value) URL.revokeObjectURL(pdfUrl.value);
		pdfUrl.value = null;
	};
	const releaseContent = () => {
		contentRequest++;
		contentAbort?.abort();
		contentAbort = null;
		releasePdf();
		textContent.value = '';
		contentState.value = 'idle';
	};
	// Bytes always come through the API with the bearer token, never through a signed link, and the PDF
	// is re-wrapped with a forced type so a blob of another type can't be framed as a document.
	const loadContent = async () => {
		const file = current.value;
		releaseContent();
		const request = contentRequest;
		if (!file || !open.value) return;
		if (kind.value === 'image') {
			void ensureUrl(file);
			return;
		}
		imageRequest++;
		imageError.value = false;
		const textual = kind.value === 'markdown' || kind.value === 'text';
		if (
			kind.value === 'other' ||
			(kind.value === 'pdf' && navigator.pdfViewerEnabled !== true) ||
			(textual && !withinTextPreviewLimit(file.size))
		) {
			contentState.value = 'unsupported';
			return;
		}
		contentState.value = 'loading';
		contentAbort = new AbortController();
		try {
			const blob = await fetchFileBlob(file.id, contentAbort.signal);
			if (disposed || request !== contentRequest) return;
			if (textual) {
				if (!withinTextPreviewLimit(blob.size)) {
					contentState.value = 'unsupported';
					return;
				}
				const text = await blob.text();
				if (disposed || request !== contentRequest) return;
				textContent.value = text;
			} else {
				pdfUrl.value = URL.createObjectURL(
					new Blob([blob], { type: 'application/pdf' }),
				);
			}
			contentState.value = 'ready';
		} catch {
			if (!disposed && request === contentRequest) contentState.value = 'error';
		}
	};

	const step = (delta: number) => {
		index.value = stepIndex(index.value, props.images.length, delta);
	};

	const onKeydown = (event: KeyboardEvent) => {
		if (event.ctrlKey || event.metaKey || event.altKey) {
			return;
		}
		// A focused text or markdown panel keeps the arrows for scrolling wide content.
		if ((event.target as Element | null)?.closest?.('[data-viewer-scroll]')) {
			return;
		}
		if (event.key === 'ArrowLeft') {
			step(-1);
		} else if (event.key === 'ArrowRight') {
			step(1);
		} else if (kind.value === 'image') {
			if (event.key === '+' || event.key === '=') {
				setZoom(zoomIn(zoom.value));
			} else if (event.key === '-' || event.key === '_') {
				setZoom(zoomOut(zoom.value));
			} else if (event.key === '0') {
				fit();
			}
		}
	};

	const onOpenChange = (value: boolean) => {
		if (!value) {
			emit('close');
		}
	};

	// Not the displayed URL: that may be a signed link on the API origin, and a browser ignores
	// `download` on a cross-origin href, opening the image instead of saving it.
	const downloading = ref(false);
	const downloadError = ref(false);
	const download = async () => {
		const file = current.value;
		if (!file || downloading.value) {
			return;
		}
		downloading.value = true;
		downloadError.value = false;
		try {
			const url = await fetchFileObjectUrl(file.id);
			const link = document.createElement('a');
			link.href = url;
			link.download = file.name;
			link.click();
			setTimeout(() => URL.revokeObjectURL(url), 10000);
		} catch {
			downloadError.value = current.value?.id === file.id;
		} finally {
			downloading.value = false;
		}
	};

	const releaseOwnUrls = () => {
		imageRequest++;
		Object.values(ownUrls.value).forEach(releaseFileDisplayUrl);
		fullLoaded.value = {};
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
			document.addEventListener('keydown', onKeydown);
		},
		{ immediate: true },
	);

	watch(
		() => [open.value, current.value?.id] as const,
		() => {
			fit();
			downloadError.value = false;
			void loadContent();
		},
		{ immediate: true },
	);

	onBeforeUnmount(() => {
		disposed = true;
		imageRequest++;
		document.removeEventListener('keydown', onKeydown);
		releaseOwnUrls();
		releaseContent();
	});
</script>
