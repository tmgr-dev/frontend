<template>
	<Dialog :open="true" @update:open="(open) => !open && $emit('cancel')">
		<DialogContent
			class="rounded-card border border-line bg-surface text-ink sm:max-w-[360px]"
		>
			<DialogHeader>
				<DialogTitle class="text-ink">Crop avatar</DialogTitle>
				<DialogDescription class="text-ink-muted">
					Drag to move, scroll or use the slider to zoom.
				</DialogDescription>
			</DialogHeader>

			<div
				class="relative mx-auto touch-none select-none overflow-hidden rounded-md bg-surface-sunken"
				:class="dragging ? 'cursor-grabbing' : 'cursor-grab'"
				:style="{ width: `${VIEW}px`, height: `${VIEW}px` }"
				@pointerdown="onPointerDown"
				@pointermove="onPointerMove"
				@pointerup="onPointerUp"
				@pointercancel="onPointerUp"
				@wheel.prevent="onWheel"
			>
				<img
					v-if="natural"
					:src="url"
					alt=""
					draggable="false"
					class="pointer-events-none absolute left-0 top-0 max-w-none"
					:style="imageStyle"
				/>
				<div
					class="pointer-events-none absolute inset-0 rounded-full shadow-[0_0_0_9999px_rgba(0,0,0,0.55)] ring-2 ring-white/80 dark:shadow-[0_0_0_9999px_rgba(0,0,0,0.7)] dark:ring-white/60"
				/>
			</div>

			<div class="flex items-center gap-3">
				<label for="avatar-crop-zoom" class="text-sm text-ink-muted">
					Zoom
				</label>
				<input
					id="avatar-crop-zoom"
					type="range"
					min="1"
					:max="MAX_ZOOM"
					step="0.01"
					:value="zoom"
					:disabled="!natural"
					class="h-1 flex-1 accent-primary"
					@input="setZoom(Number(($event.target as HTMLInputElement).value))"
				/>
			</div>

			<DialogFooter class="gap-2 sm:gap-2">
				<Button type="button" variant="outline" @click="$emit('cancel')">
					Cancel
				</Button>
				<Button
					type="button"
					:disabled="!natural || saving"
					data-testid="avatar-crop-confirm"
					@click="confirm"
				>
					Confirm
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		clampOffset,
		clampZoom,
		coverScale,
		MAX_ZOOM,
		type Offset,
		outputName,
		outputSize,
		outputType,
		sourceRect,
	} from '@/utils/avatarCrop';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		type PropType,
		ref,
	} from 'vue';

	const VIEW = 256;

	export default defineComponent({
		name: 'AvatarCropModal',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
		},
		props: {
			file: { type: Object as PropType<File>, required: true },
		},
		emits: {
			confirm: (_file: File) => true,
			cancel: () => true,
			error: () => true,
		},
		setup(props, { emit }) {
			const url = URL.createObjectURL(props.file);
			const image = new Image();
			const natural = ref<{ width: number; height: number } | null>(null);
			const zoom = ref(1);
			const offset = ref<Offset>({ x: 0, y: 0 });
			const dragging = ref(false);
			const saving = ref(false);
			let drag: { id: number; x: number; y: number; from: Offset } | null =
				null;

			image.onload = () => {
				if (!image.naturalWidth || !image.naturalHeight) {
					emit('error');
					return;
				}
				natural.value = {
					width: image.naturalWidth,
					height: image.naturalHeight,
				};
			};
			image.onerror = () => emit('error');
			image.src = url;

			onBeforeUnmount(() => {
				image.onload = null;
				image.onerror = null;
				URL.revokeObjectURL(url);
			});

			const scale = computed(() =>
				natural.value
					? coverScale(natural.value.width, natural.value.height, VIEW) *
						zoom.value
					: 1,
			);

			const clamp = (next: Offset) =>
				natural.value
					? clampOffset(
							next,
							natural.value.width,
							natural.value.height,
							scale.value,
							VIEW,
						)
					: next;

			const imageStyle = computed(() => {
				if (!natural.value) return {};
				const width = natural.value.width * scale.value;
				const height = natural.value.height * scale.value;
				return {
					width: `${width}px`,
					height: `${height}px`,
					transform: `translate(${(VIEW - width) / 2 + offset.value.x}px, ${
						(VIEW - height) / 2 + offset.value.y
					}px)`,
				};
			});

			const setZoom = (value: number) => {
				const before = scale.value;
				zoom.value = clampZoom(value);
				const ratio = scale.value / before;
				offset.value = clamp({
					x: offset.value.x * ratio,
					y: offset.value.y * ratio,
				});
			};

			const onWheel = (event: WheelEvent) => {
				if (!natural.value) return;
				setZoom(zoom.value * Math.exp(-event.deltaY * 0.002));
			};

			const onPointerDown = (event: PointerEvent) => {
				if (!natural.value || drag || event.button !== 0) return;
				(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
				drag = {
					id: event.pointerId,
					x: event.clientX,
					y: event.clientY,
					from: offset.value,
				};
				dragging.value = true;
			};

			const onPointerMove = (event: PointerEvent) => {
				if (!drag || event.pointerId !== drag.id) return;
				offset.value = clamp({
					x: drag.from.x + event.clientX - drag.x,
					y: drag.from.y + event.clientY - drag.y,
				});
			};

			const onPointerUp = (event: PointerEvent) => {
				if (!drag || event.pointerId !== drag.id) return;
				const target = event.currentTarget as HTMLElement;
				if (target.hasPointerCapture(event.pointerId)) {
					target.releasePointerCapture(event.pointerId);
				}
				drag = null;
				dragging.value = false;
			};

			const confirm = () => {
				if (!natural.value || saving.value) return;
				saving.value = true;
				const { sx, sy, size } = sourceRect(
					offset.value,
					natural.value.width,
					natural.value.height,
					scale.value,
					VIEW,
				);
				const side = outputSize(size);
				const type = outputType(props.file.type);
				const canvas = document.createElement('canvas');
				canvas.width = side;
				canvas.height = side;
				const context = canvas.getContext('2d');
				if (!context) {
					emit('error');
					return;
				}
				context.imageSmoothingQuality = 'high';
				context.drawImage(image, sx, sy, size, size, 0, 0, side, side);
				try {
					canvas.toBlob(
						(blob) => {
							if (!blob) {
								emit('error');
								return;
							}
							emit('confirm', new File([blob], outputName(type), { type }));
						},
						type,
						0.9,
					);
				} catch {
					emit('error');
				}
			};

			return {
				VIEW,
				MAX_ZOOM,
				url,
				natural,
				zoom,
				dragging,
				saving,
				imageStyle,
				setZoom,
				onWheel,
				onPointerDown,
				onPointerMove,
				onPointerUp,
				confirm,
			};
		},
	});
</script>
