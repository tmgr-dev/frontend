<template>
	<div
		:class="['dcard', 'spawn', status.tone, { resolved }]"
		:data-card="cardId"
		:data-kind="view.kind"
	>
		<div v-if="resolved" class="sp-resolved">
			<Icon :name="outcomeIcon" />
			<b>{{ $t(`manager.spawn.resolved.${statusText.outcome}`) }}</b>
			<span class="sp-sep">·</span>
			<span class="sp-rtitle">{{ title }}</span>
		</div>
		<div v-else class="dc-title"><Icon name="shield" />{{ title }}</div>
		<div v-if="!resolved && view.chips.length" class="sp-chips" role="group">
			<span
				v-for="chip in view.chips"
				:key="chip.key"
				:class="['sp-chip', chip.tone]"
				><Icon name="alert" />{{ $t(`manager.spawn.chip.${chip.key}`) }}</span
			>
		</div>
		<div v-if="!resolved && view.target" class="sp-target">
			<b class="mono">{{ view.target.label }}</b>
			<span class="sp-machine">{{ view.target.machine }}</span>
			<span :class="['pill', pillClass]"
				><Icon :name="pillIcon" />{{
					$t(`manager.spawn.state.${failed ? 'failed' : statusText.outcome}`)
				}}</span
			>
		</div>
		<div v-if="!resolved && view.structured" class="sp-proposer">
			<Icon name="info" />{{ proposer }}
		</div>
		<dl v-if="!resolved && view.lines.length" class="sp-lines">
			<template v-for="line in view.lines" :key="line.key">
				<dt>{{ $t(`manager.spawn.${line.key}`) }}</dt>
				<dd :class="{ 'sp-warn': line.warn }">
					{{ line.i18n ? $t(line.i18n) : line.value }}
				</dd>
			</template>
		</dl>
		<div class="sp-payload-box">
			<div class="sp-payload-label">{{ $t('manager.spawn.payload') }}</div>
			<pre class="sp-payload">{{ view.payload }}</pre>
		</div>
		<div v-if="showActions" class="sp-actions">
			<form v-if="editing" class="sp-edit" @submit.prevent="submitEdit">
				<label
					>{{ $t('manager.spawn.editFolder') }}
					<input v-model="form.cwd" name="cwd" type="text" required
				/></label>
				<label
					>{{ $t('manager.spawn.editMode') }}
					<select v-model="form.mode" name="mode">
						<option v-for="m in modes" :key="m" :value="m">{{ m }}</option>
					</select></label
				>
				<label
					>{{ $t('manager.spawn.editName') }}
					<input v-model="form.name" name="name" type="text"
				/></label>
				<label
					>{{ $t('manager.spawn.editTask') }}
					<input v-model="form.task" name="task" type="text"
				/></label>
				<div class="sp-row">
					<button type="submit" class="sp-btn primary" :disabled="!canAct">
						{{ $t('manager.spawn.submit') }}
					</button>
					<button
						type="button"
						class="sp-btn"
						:disabled="busy"
						@click="editing = false"
					>
						{{ $t('manager.spawn.cancel') }}
					</button>
				</div>
			</form>
			<div v-else class="sp-row">
				<button
					type="button"
					class="sp-btn primary"
					:disabled="!canAct"
					@click="decide('approve')"
				>
					{{ $t('manager.spawn.approve') }}
				</button>
				<button
					v-if="view.kind === 'spawn' && view.structured"
					type="button"
					class="sp-btn"
					:disabled="!canAct"
					@click="openEdit"
				>
					{{ $t('manager.spawn.edit') }}
				</button>
				<button
					type="button"
					class="sp-btn"
					:disabled="!canAct"
					@click="decide('reject')"
				>
					{{ $t('manager.spawn.reject') }}
				</button>
			</div>
			<div v-if="outdated" class="sp-outdated">
				{{ $t('manager.spawn.outdated') }}
			</div>
		</div>
		<div v-if="decideError" class="dc-note dc-err" role="alert">
			{{ decideError }}
		</div>
		<div v-else-if="replaced" class="dc-note sp-status">
			{{ $t('manager.spawn.replacedNote') }}
		</div>
		<div v-if="failed" class="dc-note dc-err" role="alert">
			{{ $t('manager.spawn.unavailable') }}
		</div>
		<div v-else-if="current" class="dc-note sp-status">
			{{ $t(statusText.key, statusText.params) }}
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { runnerCaps, spawnCard, spawnDecide } from '../api';
	import {
		SPAWN_MODES,
		buildEdit,
		canDecide,
		isOutdatedError,
		parseSpawnRequest,
		spawnStatusText,
		spawnView,
		supportsDecide,
	} from '../spawnCard';
	import type { SpawnCardStatus, SpawnCardWire, SpawnEdit } from '../types';
	import Icon from './Icon.vue';

	const POLL_MS = 2000;
	const TICK_MS = 1000;

	export default defineComponent({
		name: 'ManagerSpawnApprovalCard',
		components: { Icon },
		props: {
			cardId: { type: String, required: true },
			spawn: { type: Object as PropType<SpawnCardWire>, required: true },
		},
		data() {
			return {
				current: null as SpawnCardStatus | null,
				failed: false,
				now: Date.now(),
				poll: 0 as unknown as ReturnType<typeof setInterval>,
				tick: 0 as unknown as ReturnType<typeof setInterval>,
				gone: false,
				caps: null as string[] | null,
				outdatedSeen: false,
				busy: false,
				editing: false,
				replaced: false,
				decideError: '',
				modes: SPAWN_MODES as readonly string[],
				form: { cwd: '', mode: 'ask', name: '', task: '' },
			};
		},
		computed: {
			view() {
				return spawnView(this.spawn);
			},
			statusText() {
				return spawnStatusText(
					this.current ?? { state: 'issued' },
					this.spawn.exp_ms,
					this.now,
				);
			},
			title(): string {
				const t = this.view.title;
				return t ? this.$t(t.key, t.params) : this.view.summary;
			},
			proposer(): string {
				const p = this.view.proposer;
				return p?.key === 'session'
					? this.$t('manager.spawn.requestedBySession', {
							session: p.session ?? '',
						})
					: this.$t('manager.spawn.proposedByManager');
			},
			resolved(): boolean {
				return !this.failed && !!this.current && this.statusText.final;
			},
			pillClass(): string {
				return { wait: 'st-waiting', ok: 'st-done', fail: 'st-failed' }[
					this.status.tone
				] as string;
			},
			pillIcon(): string {
				return { wait: 'hand', ok: 'check', fail: 'x' }[
					this.status.tone
				] as string;
			},
			outcomeIcon(): string {
				return this.statusText.outcome === 'approved' ? 'check' : 'x';
			},
			showActions(): boolean {
				return (
					!this.failed &&
					!!this.current &&
					canDecide(this.current, this.spawn.exp_ms, this.now)
				);
			},
			outdated(): boolean {
				return (
					this.outdatedSeen ||
					(this.caps !== null && !supportsDecide(this.caps))
				);
			},
			canAct(): boolean {
				return !this.busy && !this.outdated && this.caps !== null;
			},
			status() {
				return { tone: this.failed ? 'fail' : this.statusText.tone };
			},
		},
		mounted() {
			void this.refresh();
			runnerCaps().then(
				(caps) => {
					if (!this.gone) this.caps = caps;
				},
				() => undefined,
			);
			this.poll = setInterval(() => void this.refresh(), POLL_MS);
			this.tick = setInterval(() => (this.now = Date.now()), TICK_MS);
		},
		beforeUnmount() {
			this.gone = true;
			this.stop();
		},
		methods: {
			stop() {
				clearInterval(this.poll);
				clearInterval(this.tick);
			},
			openEdit() {
				const p = parseSpawnRequest(this.spawn.request);
				this.form = {
					cwd: p?.cwd ?? '',
					mode: this.modes.includes(p?.mode ?? '')
						? (p?.mode as string)
						: 'ask',
					name: p?.name ?? '',
					task: p?.task ?? '',
				};
				this.decideError = '';
				this.editing = true;
			},
			submitEdit() {
				const edit = buildEdit(this.form);
				if (edit) void this.decide('edit', edit);
			},
			async decide(decision: 'approve' | 'reject' | 'edit', edit?: SpawnEdit) {
				if (this.busy) return;
				this.busy = true;
				this.decideError = '';
				try {
					const out = await spawnDecide(
						this.cardId,
						this.spawn.request,
						decision,
						edit,
					);
					if (decision === 'edit' && out?.card_id) this.replaced = true;
					this.editing = false;
				} catch (e) {
					if (isOutdatedError(e)) this.outdatedSeen = true;
					else this.decideError = String(e instanceof Error ? e.message : e);
				} finally {
					this.busy = false;
				}
				await this.refresh();
			},
			async refresh() {
				try {
					const status = await spawnCard(this.cardId);
					if (this.gone) return;
					this.current = status;
					this.failed = false;
					this.now = Date.now();
					if (this.statusText.final) this.stop();
				} catch {
					if (!this.gone) this.failed = true;
				}
			},
		},
	});
</script>

<style scoped>
	.sp-actions {
		margin-top: 10px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.sp-row {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.sp-btn {
		padding: 5px 14px;
		border-radius: 8px;
		border: 1px solid var(--border, rgba(127, 127, 127, 0.35));
		background: transparent;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}
	.sp-btn.primary {
		background: var(--accent, #3b82f6);
		border-color: transparent;
		color: #fff;
	}
	.sp-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.sp-edit {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.sp-edit label {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 12px;
		opacity: 0.9;
	}
	.sp-edit input,
	.sp-edit select {
		font: inherit;
		padding: 4px 8px;
		border-radius: 6px;
		border: 1px solid var(--border, rgba(127, 127, 127, 0.35));
		background: transparent;
		color: inherit;
	}
	.sp-outdated {
		font-size: 12px;
		opacity: 0.75;
	}
</style>
