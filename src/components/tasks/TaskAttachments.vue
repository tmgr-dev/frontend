<template>
	<div class="task-attachments mt-4">
		<div class="mb-3 flex items-center justify-between">
			<h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300">
				Attachments
			</h3>
			<button
				@click="handleAddFiles"
				class="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-blue-600 transition-colors hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-gray-800"
			>
				<Paperclip :size="16" />
				<span>Add files</span>
			</button>
		</div>

		<div v-if="files.length > 0" class="mb-3 space-y-2">
			<div
				v-for="(file, index) in files"
				:key="index"
				class="flex items-center gap-3 rounded-lg border border-gray-200 bg-gray-50 p-3 transition-colors hover:border-gray-300 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-gray-600"
			>
				<div class="flex-shrink-0">
					<img
						v-if="file.preview"
						:src="file.preview"
						:alt="file.name"
						class="h-12 w-12 rounded object-cover"
					/>
					<FileIcon
						v-else
						:size="20"
						class="text-gray-500 dark:text-gray-400"
					/>
				</div>
				<div class="min-w-0 flex-1">
					<p
						class="truncate text-sm font-medium text-gray-900 dark:text-gray-100"
					>
						{{ file.name }}
					</p>
					<p class="text-xs text-gray-500 dark:text-gray-400">
						{{ formatFileSize(file.size) }}
					</p>
				</div>
				<button
					@click="handleRemoveFile(index)"
					class="flex-shrink-0 rounded p-1.5 text-gray-400 transition-colors hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-900/20 dark:hover:text-red-400"
				>
					<X :size="16" />
				</button>
			</div>
		</div>

		<div
			@click="handleAddFiles"
			@dragover.prevent="handleDragOver"
			@dragleave.prevent="handleDragLeave"
			@drop.prevent="handleDrop"
			:class="[
				'cursor-pointer rounded-lg border-2 border-dashed p-6 text-center transition-colors',
				isDragOver
					? 'border-blue-500 bg-blue-50 dark:border-blue-400 dark:bg-blue-900/20'
					: 'border-gray-300 hover:border-gray-400 hover:bg-gray-50 dark:border-gray-600 dark:hover:border-gray-500 dark:hover:bg-gray-800/50',
			]"
		>
			<FileIcon
				:size="32"
				class="mx-auto mb-2 text-gray-400 dark:text-gray-500"
			/>
			<p class="text-sm text-gray-500 dark:text-gray-400">
				{{ files.length === 0 ? 'No attachments yet' : 'Add more files' }}
			</p>
			<p class="mt-1 text-xs text-gray-400 dark:text-gray-500">
				Click or drag files here to attach documents, images, or other files
			</p>
		</div>

		<input
			ref="fileInput"
			type="file"
			multiple
			class="hidden"
			@change="handleFileSelect"
		/>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';
	import { Paperclip, FileIcon, X } from 'lucide-vue-next';

	export default defineComponent({
		name: 'TaskAttachments',
		components: {
			Paperclip,
			FileIcon,
			X,
		},
		props: {
			taskId: {
				type: Number,
				required: false,
				default: null,
			},
		},
		data() {
			return {
				files: [],
				isDragOver: false,
			};
		},
		methods: {
			handleAddFiles() {
				this.$refs.fileInput.click();
			},
			isImageFile(file) {
				return file.type.startsWith('image/');
			},
			createFilePreview(file) {
				if (this.isImageFile(file)) {
					return URL.createObjectURL(file);
				}
				return null;
			},
			handleFileSelect(event) {
				const selectedFiles = Array.from(event.target.files);
				selectedFiles.forEach((file) => {
					file.preview = this.createFilePreview(file);
					this.files.push(file);
				});
				event.target.value = '';
			},
			handleRemoveFile(index) {
				const file = this.files[index];
				if (file.preview) {
					URL.revokeObjectURL(file.preview);
				}
				this.files.splice(index, 1);
			},
			handleDragOver(event) {
				event.preventDefault();
				this.isDragOver = true;
			},
			handleDragLeave(event) {
				event.preventDefault();
				if (!event.currentTarget.contains(event.relatedTarget)) {
					this.isDragOver = false;
				}
			},
			handleDrop(event) {
				event.preventDefault();
				this.isDragOver = false;
				const droppedFiles = Array.from(event.dataTransfer.files);
				if (droppedFiles.length > 0) {
					droppedFiles.forEach((file) => {
						file.preview = this.createFilePreview(file);
						this.files.push(file);
					});
				}
			},
			formatFileSize(bytes) {
				if (bytes === 0) return '0 Bytes';
				const k = 1024;
				const sizes = ['Bytes', 'KB', 'MB', 'GB'];
				const i = Math.floor(Math.log(bytes) / Math.log(k));
				return (
					Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
				);
			},
		},
		unmounted() {
			this.files.forEach((file) => {
				if (file.preview) {
					URL.revokeObjectURL(file.preview);
				}
			});
		},
	});
</script>

<style lang="scss" scoped>
	.task-attachments {
		width: 100%;
	}
</style>
