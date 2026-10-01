<template>
	<div
		data-testid="page-files"
		:class="
			dragging
				? 'rounded-md ring-2 ring-blue-400 ring-offset-2 dark:ring-offset-gray-900'
				: ''
		"
		@dragover.prevent="dragging = true"
		@dragleave="dragging = false"
		@drop.prevent="onDrop"
	>
		<div class="mb-2 flex items-center justify-between">
			<h2 class="text-xs font-semibold uppercase tracking-wide text-ink-subtle">
				Files
			</h2>
			<button
				type="button"
				class="text-xs text-blue-600 hover:underline disabled:opacity-50 dark:text-blue-400"
				:disabled="uploading"
				data-testid="page-files-upload"
				@click="pick"
			>
				{{ uploading ? 'Uploading...' : 'Upload' }}
			</button>
			<input
				ref="inputRef"
				type="file"
				multiple
				class="hidden"
				@change="onPick"
			/>
		</div>
		<ul v-if="files.length" class="space-y-1">
			<li v-for="file in files" :key="file.id" class="flex items-center gap-2">
				<button
					type="button"
					class="min-w-0 flex-1 truncate text-left text-blue-600 hover:underline dark:text-blue-400"
					:title="file.original_name || file.name"
					@click="$emit('open', file)"
				>
					{{ file.original_name || file.name }}
				</button>
				<span class="shrink-0 text-xs text-gray-500 dark:text-gray-400">{{
					sizeOf(file.size)
				}}</span>
			</li>
		</ul>
		<p v-else class="text-gray-400 dark:text-gray-500">None</p>
		<p
			v-if="error"
			class="mt-1 text-xs text-red-600 dark:text-red-400"
			role="alert"
		>
			{{ error }}
		</p>
	</div>
</template>

<script lang="ts">
	import type { PageFile } from '@/actions/tmgr/pages';
	import { formatFileSize } from '@/utils/pages/files';
	import { defineComponent, type PropType, ref } from 'vue';

	export default defineComponent({
		name: 'PageFilesPanel',
		props: {
			files: { type: Array as PropType<PageFile[]>, default: () => [] },
			uploading: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['upload', 'open'],
		setup(_props, { emit }) {
			const inputRef = ref<HTMLInputElement | null>(null);
			const dragging = ref(false);
			const pick = () => inputRef.value?.click();
			const onPick = (event: Event) => {
				const input = event.target as HTMLInputElement;
				if (input.files?.length) emit('upload', Array.from(input.files));
				input.value = '';
			};
			const onDrop = (event: DragEvent) => {
				dragging.value = false;
				const list = Array.from(event.dataTransfer?.files ?? []);
				if (list.length) emit('upload', list);
			};
			return {
				inputRef,
				dragging,
				pick,
				onPick,
				onDrop,
				sizeOf: formatFileSize,
			};
		},
	});
</script>
