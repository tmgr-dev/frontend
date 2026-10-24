<template>
	<Modal
		modal-class="w-[420px] p-6"
		@close="$emit('close')"
		@closingModal="$emit('close')"
	>
		<template #modal-body>
			<div class="text-ink">
				<div class="flex items-center justify-between">
					<div class="text-xl font-bold">Batch Actions</div>
					<a
						class="text-ink-subtle hover:text-ink"
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
						class="flex-1 rounded-md border border-line-strong bg-surface px-3 py-2 text-sm text-ink outline-hidden"
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
						<optgroup v-if="agentRunsOn" label="Agent runs">
							<option value="agent-confirm">Confirm for agent runs</option>
						</optgroup>
						<optgroup label="Export">
							<option value="export:csv">Export to CSV</option>
							<option value="export:jpg">Export to JPEG</option>
							<option value="export:xlsx">Export to XLSX</option>
						</optgroup>
					</select>
					<button
						data-tour="export.download"
						class="flex items-center gap-2 rounded-md bg-status-done px-4 py-2 text-sm font-medium text-white shadow-tmgr-xs hover:opacity-90"
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
	import { getAgentRunsSetting } from '@/actions/tmgr/agentRuns';
	import Modal from '@/components/Modal.vue';
	import ExportSettingsPanel from '@/components/tasks/ExportSettingsPanel.vue';
	import { archiveTarget, doneTarget, openTarget } from '@/utils/simpleStatus';

	export default {
		name: 'TasksMultipleActionsModal',
		emits: ['export', 'remove', 'updateStatus', 'close', 'confirmAgentRuns'],
		components: {
			Modal,
			ExportSettingsPanel,
		},
		data() {
			return {
				exportSettings: {},
				selectedAction: 'export:csv',
				elapsedTime: 0,
				timerInterval: null,
				agentRunsOn: false,
			};
		},
		async mounted() {
			const id = Number(this.$store.getters.currentWorkspaceId);
			if (!(id > 0) || this.$store.getters.isLocalOnly) return;
			try {
				this.agentRunsOn = await getAgentRunsSetting(id);
			} catch {
				this.agentRunsOn = false;
			}
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
			exportsEnabled() {
				return (
					!this.$store.getters.isLocalOnly &&
					!!this.$store.getters['featureToggles/modulesKnown'] &&
					this.$store.getters['featureToggles/isFeatureEnabled']('exports')
				);
			},
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
				} else if (action === 'agent-confirm') {
					this.$emit('confirmAgentRuns');
				} else if (action === 'delete') {
					this.$emit('remove');
				}
			},
		},
	};
</script>
