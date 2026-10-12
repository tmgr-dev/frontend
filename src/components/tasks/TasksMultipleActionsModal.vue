<template>
	<Modal
		modal-class="w-[420px] p-6"
		@close="$emit('close')"
		@closingModal="$emit('close')"
	>
		<template #modal-body>
			<div class="text-tmgr-blue dark:text-gray-300">
				<div class="flex items-center justify-between">
					<div class="text-xl font-bold">Batch Actions</div>
					<a
						class="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
						href="#close"
						@click.prevent="$emit('close')"
					>
						<span class="material-icons">close</span>
					</a>
				</div>

				<slot></slot>

				<export-settings-panel
					v-if="exportsEnabled"
					data-tour="export.panel"
					@update:settings="exportSettings = $event"
				/>

				<div class="mt-4 flex items-center gap-2">
					<select
						v-model="selectedAction"
						class="flex-1 rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 outline-none focus:border-green-500 focus:ring-1 focus:ring-green-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
					>
						<optgroup v-if="simpleMode" label="Update tasks">
							<option
								:value="`status:${doneStatusId}`"
								:disabled="!doneStatusId"
							>
								{{ doneStatusId ? 'Mark done' : 'Mark done (no Done status)' }}
							</option>
							<option
								:value="`status:${openStatusId}`"
								:disabled="!openStatusId"
							>
								Mark open
							</option>
							<option
								v-if="archiveStatusId"
								:value="`status:${archiveStatusId}`"
							>
								Archive
							</option>
							<option
								v-if="status === 'hidden' || status === 'done'"
								value="delete"
							>
								Delete
							</option>
						</optgroup>
						<optgroup v-else label="Move to status">
							<option
								v-for="workspaceStatus in statuses"
								:key="workspaceStatus.id"
								:value="`status:${workspaceStatus.id}`"
							>
								{{ workspaceStatus.name }}
							</option>
							<option
								v-if="status === 'hidden' || status === 'done'"
								value="delete"
							>
								Delete
							</option>
						</optgroup>
						<optgroup label="Export">
							<option value="export:csv">Export to CSV</option>
							<option value="export:jpg">Export to JPEG</option>
							<option value="export:xlsx">Export to XLSX</option>
						</optgroup>
					</select>
					<button
						data-tour="export.download"
						class="flex items-center gap-2 rounded-md bg-green-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-offset-2"
						:disabled="isLoading"
						@click="executeAction"
					>
						<template v-if="isLoading">
							<loader is-mini />
							<span>{{ elapsedTime }}s</span>
						</template>
						<span v-else>Execute</span>
					</button>
				</div>
			</div>
		</template>
	</Modal>
</template>

<script>
	import Modal from '@/components/Modal.vue';
	import ExportSettingsPanel from '@/components/tasks/ExportSettingsPanel.vue';
	import { archiveTarget, doneTarget, openTarget } from '@/utils/simpleStatus';

	export default {
		name: 'TasksMultipleActionsModal',
		emits: ['export', 'remove', 'updateStatus', 'close'],
		components: {
			Modal,
			ExportSettingsPanel,
		},
		computed: {
			exportsEnabled() {
				return this.$store.getters['featureToggles/isFeatureEnabled'](
					'exports',
				);
			},
		},
		data() {
			return {
				exportSettings: {},
				selectedAction: 'export:csv',
				elapsedTime: 0,
				timerInterval: null,
			};
		},
		props: {
			status: {
				required: false,
				type: String,
				default: null,
			},
			statuses: {
				type: Array,
				required: false,
				default: () => [],
			},
			isLoadingActions: {
				type: Object,
				required: true,
			},
			simpleMode: {
				type: Boolean,
				default: false,
			},
		},
		computed: {
			doneStatusId() {
				return doneTarget(this.statuses);
			},
			openStatusId() {
				return openTarget(this.statuses);
			},
			archiveStatusId() {
				return archiveTarget(this.statuses);
			},
			isLoading() {
				return this.isLoadingActions.length > 0;
			},
		},
		watch: {
			isLoading(loading) {
				if (loading) {
					this.elapsedTime = 0;
					this.timerInterval = setInterval(() => {
						this.elapsedTime++;
					}, 1000);
				} else {
					clearInterval(this.timerInterval);
					this.timerInterval = null;
				}
			},
		},
		beforeUnmount() {
			clearInterval(this.timerInterval);
		},
		methods: {
			executeAction() {
				const action = this.selectedAction;
				if (action.startsWith('status:')) {
					this.$emit('updateStatus', action.replace('status:', ''));
				} else if (action.startsWith('export:')) {
					const type = action.replace('export:', '');
					this.$emit('export', {
						type,
						settings: this.exportSettings,
					});
				} else if (action === 'delete') {
					this.$emit('remove');
				}
			},
		},
	};
</script>
