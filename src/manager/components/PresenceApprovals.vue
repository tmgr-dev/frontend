<template>
	<div v-if="status?.available" class="blk pres">
		<h4>{{ $t('manager.presence.title') }}</h4>
		<p>{{ $t('manager.presence.body') }}</p>
		<p v-if="status.needs_setup_again" class="hint pres-again" role="alert">
			{{ $t('manager.presence.setupAgain') }}
		</p>
		<p v-else-if="!status.set_up" class="hint pres-none">
			{{ $t('manager.presence.notSetUp') }}
		</p>
		<template v-else>
			<div class="fp">
				<span>{{ $t('manager.presence.keyFp') }}</span
				><b class="mono pres-fp">{{ keyGrouped }}</b>
			</div>
			<p v-if="status.unlocked_secs" class="hint pres-open">
				{{
					$t('manager.presence.unlocked', {
						minutes: Math.max(1, Math.ceil(status.unlocked_secs / 60)),
					})
				}}
			</p>
		</template>
		<p v-if="sentTo !== null" class="hint pres-sent">
			{{ $t('manager.presence.sent', { count: sentTo }) }}
		</p>
		<template v-if="status.set_up && selfFp && status.key_fp">
			<p class="hint">{{ $t('manager.presence.approve') }}</p>
			<pre class="mono pres-cmd">{{ approveCommand }}</pre>
		</template>
		<div class="pres-actions">
			<button
				v-if="!status.set_up"
				type="button"
				class="btn sm pres-setup"
				:disabled="busy"
				@click="run(presenceSetup)"
			>
				{{ $t('manager.presence.setup') }}
			</button>
			<template v-else>
				<button
					type="button"
					class="btn sm pres-resend"
					:disabled="busy"
					@click="run(presencePublish)"
				>
					{{ $t('manager.presence.resend') }}
				</button>
				<button
					v-if="status.unlocked_secs"
					type="button"
					class="btn sm pres-lock"
					:disabled="busy"
					@click="lock"
				>
					{{ $t('manager.presence.lock') }}
				</button>
			</template>
		</div>
		<p v-if="error" class="hint dc-err" role="alert">
			{{ $t('manager.presence.failed', { error }) }}
		</p>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import {
		presenceLock,
		presencePublish,
		presenceSetup,
		presenceStatus,
	} from '../api';
	import type { PresenceStatus } from '../types';
	import { groupFp } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerPresenceApprovals',
		props: {
			selfFp: { type: String, default: '' },
		},
		data() {
			return {
				status: null as PresenceStatus | null,
				busy: false,
				error: '',
				sentTo: null as number | null,
			};
		},
		computed: {
			keyGrouped(): string {
				return groupFp(this.status?.key_fp ?? '');
			},
			approveCommand(): string {
				return `tmgr runner manager presence-keys approve ${this.selfFp} ${
					this.status?.key_fp ?? ''
				}`;
			},
		},
		mounted() {
			this.load();
		},
		methods: {
			presenceSetup,
			presencePublish,
			async load() {
				try {
					this.status = await presenceStatus();
				} catch {
					this.status = null;
				}
			},
			async run(action: () => Promise<number>) {
				if (this.busy) return;
				this.busy = true;
				this.error = '';
				try {
					this.sentTo = await action();
				} catch (e) {
					this.error = String(e);
				} finally {
					this.busy = false;
					await this.load();
				}
			},
			async lock() {
				await presenceLock().catch(() => undefined);
				await this.load();
			},
		},
	});
</script>

<style scoped>
	.pres-cmd {
		white-space: pre-wrap;
		word-break: break-all;
		user-select: text;
	}
	.pres-actions {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
</style>
