<template>
	<Modal
		modal-class="w-[420px] p-6"
		@close="$emit('close')"
		@closingModal="$emit('close')"
	>
		<template #modal-body>
			<div class="text-ink">
				<div class="flex items-center justify-between">
					<div class="text-xl font-bold">
						{{ $t('tasks.multipleActions.title') }}
					</div>
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
						<optgroup
							v-if="simpleMode"
							:label="$t('tasks.multipleActions.updateTasks')"
						>
							<option
								:value="`status:${doneStatusId}`"
								:disabled="!doneStatusId"
							>
								{{
									doneStatusId
										? $t('tasks.multipleActions.markDone')
										: $t('tasks.multipleActions.markDoneNoStatus')
								}}
							</option>
							<option
								:value="`status:${openStatusId}`"
								:disabled="!openStatusId"
							>
								{{ $t('tasks.multipleActions.markOpen') }}
							</option>
							<option
								v-if="archiveStatusId"
								:value="`status:${archiveStatusId}`"
							>
								{{ $t('tasks.multipleActions.archive') }}
							</option>
							<option
								v-if="status === 'hidden' || status === 'done'"
								value="delete"
							>
								{{ $t('tasks.multipleActions.delete') }}
							</option>
						</optgroup>
						<optgroup v-else :label="$t('tasks.multipleActions.moveToStatus')">
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
								{{ $t('tasks.multipleActions.delete') }}
							</option>
						</optgroup>
						<optgroup
							v-if="agentRunsOn"
							:label="$t('tasks.multipleActions.agentRuns')"
						>
							<option value="agent-confirm">
								{{ $t('tasks.multipleActions.confirmAgentRuns') }}
							</option>
						</optgroup>
						<optgroup :label="$t('tasks.multipleActions.export')">
							<option value="export:csv">
								{{ $t('tasks.multipleActions.exportCsv') }}
							</option>
							<option value="export:jpg">
								{{ $t('tasks.multipleActions.exportJpg') }}
							</option>
							<option value="export:xlsx">
								{{ $t('tasks.multipleActions.exportXlsx') }}
							</option>
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
							<span>{{
								$t('tasks.multipleActions.seconds', { n: elapsedTime })
							}}</span>
						</template>
						<span v-else>{{ $t('tasks.multipleActions.execute') }}</span>
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
