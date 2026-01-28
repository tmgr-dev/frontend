<template>
	<div class="task-attachments mt-4">
		<div class="flex items-center justify-between mb-3">
			<h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300">
				Attachments
			</h3>
			<button
				@click="handleAddFiles"
				class="flex items-center gap-1.5 px-3 py-1.5 text-sm text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-gray-800 rounded-md transition-colors"
			>
				<Paperclip :size="16" />
				<span>Add files</span>
			</button>
		</div>

		<div
			v-if="files.length === 0"
			class="border-2 border-dashed border-gray-300 dark:border-gray-600 rounded-lg p-6 text-center"
		>
			<FileIcon :size="32" class="mx-auto text-gray-400 dark:text-gray-500 mb-2" />
			<p class="text-sm text-gray-500 dark:text-gray-400">
				No attachments yet
			</p>
			<p class="text-xs text-gray-400 dark:text-gray-500 mt-1">
				Click "Add files" to attach documents, images, or other files
			</p>
		</div>

		<div v-else class="space-y-2">
			<div
				v-for="(file, index) in files"
				:key="index"
				class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600 transition-colors"
			>
				<div class="flex-shrink-0">
					<FileIcon :size="20" class="text-gray-500 dark:text-gray-400" />
				</div>
				<div class="flex-1 min-w-0">
					<p class="text-sm font-medium text-gray-900 dark:text-gray-100 truncate">
						{{ file.name }}
					</p>
					<p class="text-xs text-gray-500 dark:text-gray-400">
						{{ formatFileSize(file.size) }}
					</p>
				</div>
				<button
					@click="handleRemoveFile(index)"
					class="flex-shrink-0 p-1.5 text-gray-400 hover:text-red-500 dark:hover:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
				>
					<X :size="16" />
				</button>
			</div>
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
			};
		},
		methods: {
			handleAddFiles() {
				this.$refs.fileInput.click();
			},
			handleFileSelect(event) {
				const selectedFiles = Array.from(event.target.files);
				this.files.push(...selectedFiles);
				event.target.value = '';
			},
			handleRemoveFile(index) {
				this.files.splice(index, 1);
			},
			formatFileSize(bytes) {
				if (bytes === 0) return '0 Bytes';
				const k = 1024;
				const sizes = ['Bytes', 'KB', 'MB', 'GB'];
				const i = Math.floor(Math.log(bytes) / Math.log(k));
				return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
			},
		},
	});
</script>

<style lang="scss" scoped>
	.task-attachments {
		width: 100%;
	}
</style>
