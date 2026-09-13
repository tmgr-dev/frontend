<template>
	<div class="workspace-files mx-auto w-full max-w-5xl px-4 py-6">
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
			<h1 class="text-lg font-semibold text-gray-900 dark:text-gray-100">
				Files
				<span v-if="total" class="font-normal text-gray-400">({{ total }})</span>
			</h1>

			<div class="flex items-center gap-1 rounded-lg bg-gray-100 p-1 dark:bg-gray-800">
				<button
					v-for="option in filters"
					:key="option.value"
					type="button"
					class="rounded-md px-3 py-1 text-sm transition-colors"
					:class="
						imagesOnly === option.value
							? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100'
							: 'text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200'
					"
					@click="setFilter(option.value)"
				>
					{{ option.label }}
				</button>
			</div>
		</div>

		<div v-if="loading && !files.length" class="py-16 text-center">
			<Loader2 :size="28" class="mx-auto animate-spin text-gray-400" />
		</div>

		<div
			v-else-if="!files.length"
			class="rounded-lg border-2 border-dashed border-gray-300 py-16 text-center dark:border-gray-700"
		>
			<FileIcon :size="32" class="mx-auto mb-2 text-gray-400" />
			<p class="text-sm text-gray-500 dark:text-gray-400">
				{{
					imagesOnly
						? 'No images attached anywhere in this workspace yet'
						: 'Nothing attached anywhere in this workspace yet'
				}}
			</p>
		</div>

		<ul v-else class="space-y-2">
			<li
				v-for="file in files"
				:key="file.id"
				class="flex items-center gap-3 rounded-lg border border-gray-200 bg-gray-50 p-3 dark:border-gray-700 dark:bg-gray-800"
			>
				<div class="flex-shrink-0">
					<button
						v-if="previews[file.id]"
						type="button"
						class="block cursor-zoom-in rounded focus:outline-none focus:ring-2 focus:ring-blue-500"
						:title="`Open ${file.name}`"
						@click="galleryStartId = file.id"
					>
						<img
							:src="previews[file.id]"
							:alt="file.name"
							class="h-12 w-12 rounded object-cover"
						/>
					</button>
					<FileIcon v-else :size="20" class="text-gray-500 dark:text-gray-400" />
				</div>

				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-medium text-gray-900 dark:text-gray-100">
						{{ file.name }}
					</p>
					<p class="flex flex-wrap items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
						<span>{{ formatFileSize(file.size) }}</span>
						<span aria-hidden="true">·</span>
						<button
							type="button"
							class="truncate text-left text-blue-600 hover:underline dark:text-blue-400"
							:title="file.task.title || ''"
							@click="openTask(file)"
						>
							{{ file.task.key }} {{ taskName(file) }}
						</button>
					</p>
				</div>

				<button
					type="button"
					class="flex-shrink-0 rounded p-1.5 text-gray-400 transition-colors hover:bg-gray-200 hover:text-gray-600 dark:hover:bg-gray-700"
					title="Download"
					@click="download(file)"
				>
					<Download :size="16" />
				</button>
			</li>
		</ul>

		<div v-if="lastPage > 1" class="mt-4 flex items-center justify-center gap-3">
			<button
				type="button"
				class="rounded-md px-3 py-1.5 text-sm text-gray-600 disabled:opacity-40 dark:text-gray-300"
				:disabled="page <= 1 || loading"
				@click="goTo(page - 1)"
			>
				Previous
			</button>
			<span class="text-sm text-gray-500 dark:text-gray-400">
				{{ page }} / {{ lastPage }}
			</span>
			<button
				type="button"
				class="rounded-md px-3 py-1.5 text-sm text-gray-600 disabled:opacity-40 dark:text-gray-300"
				:disabled="page >= lastPage || loading"
				@click="goTo(page + 1)"
			>
				Next
			</button>
		</div>

		<AttachmentGallery
			:images="images"
			:start-id="galleryStartId"
			:urls="previews"
			@close="galleryStartId = null"
		/>
	</div>
</template>

<script lang="ts">
	import { Download, FileIcon, Loader2 } from 'lucide-vue-next';
	import { defineComponent } from 'vue';
	import store from '@/store';
	import {
		fetchFileObjectUrl,
		getWorkspaceFiles,
		type WorkspaceFile,
	} from '@/actions/tmgr/files';
	import { formatFileSize, isImageMime } from '@/utils/attachments';
	import { galleryImages } from '@/utils/galleryNavigation';
	import AttachmentGallery from '@/components/tasks/AttachmentGallery.vue';

	export default defineComponent({
		name: 'WorkspaceFilesPage',
		components: { AttachmentGallery, Download, FileIcon, Loader2 },
		data() {
			return {
				files: [] as WorkspaceFile[],
				previews: {} as Record<number, string>,
				imagesOnly: false,
				page: 1,
				lastPage: 1,
				total: 0,
				loading: false,
				galleryStartId: null as number | null,
				filters: [
					{ value: false, label: 'All files' },
					{ value: true, label: 'Images' },
				],
			};
		},
		computed: {
			workspaceId(): number | null {
				return store.getters.currentWorkspaceId ?? null;
			},
			images(): WorkspaceFile[] {
				return galleryImages(this.files);
			},
		},
		created() {
			this.load();
		},
		methods: {
			formatFileSize,
			taskName(file: WorkspaceFile): string {
				// The key is usually already the title's prefix; showing both would read "TM-7 TM-7: …".
				const title = file.task.title ?? '';
				const key = file.task.key ?? '';
				return key && title.startsWith(key) ? title.slice(key.length).replace(/^[:\s]+/, '') : title;
			},
			async load() {
				if (!this.workspaceId) {
					return;
				}
				this.loading = true;
				try {
					const response = await getWorkspaceFiles(this.workspaceId, {
						page: this.page,
						images: this.imagesOnly,
					});
					this.releasePreviews();
					this.files = response.data;
					this.total = response.meta.total;
					this.lastPage = response.meta.last_page;
					this.files.forEach((file) => this.loadPreview(file));
				} catch {
					this.files = [];
					this.total = 0;
					this.lastPage = 1;
				} finally {
					this.loading = false;
				}
			},
			async loadPreview(file: WorkspaceFile) {
				if (!isImageMime(file.mime_type) || this.previews[file.id]) {
					return;
				}
				try {
					this.previews[file.id] = await fetchFileObjectUrl(file.id);
				} catch {
					// A missing preview costs nothing: the row still names the file and downloads it.
				}
			},
			setFilter(imagesOnly: boolean) {
				if (this.imagesOnly === imagesOnly) {
					return;
				}
				this.imagesOnly = imagesOnly;
				this.page = 1;
				this.load();
			},
			goTo(page: number) {
				this.page = Math.min(Math.max(1, page), this.lastPage);
				this.galleryStartId = null;
				this.load();
			},
			openTask(file: WorkspaceFile) {
				store.commit('setCurrentTaskIdForModal', file.task.id);
			},
			async download(file: WorkspaceFile) {
				try {
					const url = this.previews[file.id] ?? (await fetchFileObjectUrl(file.id));
					const link = document.createElement('a');
					link.href = url;
					link.download = file.name;
					link.click();
					if (!this.previews[file.id]) {
						setTimeout(() => URL.revokeObjectURL(url), 10000);
					}
				} catch {
					// Nothing useful to say beyond leaving the row as it was.
				}
			},
			releasePreviews() {
				Object.values(this.previews).forEach((url) => URL.revokeObjectURL(url));
				this.previews = {};
			},
		},
		watch: {
			workspaceId() {
				this.page = 1;
				this.load();
			},
		},
		unmounted() {
			this.releasePreviews();
		},
	});
</script>
