<template>
	<div class="trust-overlay">
		<div
			ref="dialog"
			class="trust-dialog swb"
			role="dialog"
			aria-modal="true"
			aria-labelledby="mgr-switch-title"
			@keydown="onKey"
		>
			<h2 id="mgr-switch-title">{{ $t('manager.switchAgent.title') }}</h2>
			<div class="lbl">{{ $t('manager.switchAgent.agent') }}</div>
			<button
				v-for="a in agents"
				:key="a"
				type="button"
				:class="['agrow', { on: agent === a }]"
				:aria-pressed="agent === a"
				@click="$emit('pick', a)"
			>
				<Avatar :agent="a" />
				<span class="am2"
					><b>{{ a }}</b
					><small>{{
						a === current
							? $t('manager.switchAgent.runningNow')
							: cliOnly(a)
								? $t('manager.switchAgent.cliOnly')
								: ''
					}}</small></span
				><span class="rd"></span>
			</button>
			<div class="lbl">{{ $t('manager.switchAgent.model') }}</div>
			<div class="mods">
				<button
					v-for="m in models"
					:key="m"
					type="button"
					:class="['mchip', { on: model === m }]"
					:aria-pressed="model === m"
					@click="$emit('update:model', m)"
				>
					{{ m }}
				</button>
			</div>
			<p v-if="cliOnly(agent)" class="hint">
				{{ $t('manager.switchAgent.cliNote', { agent }) }}
			</p>
			<div class="hoff">
				<b>{{ $t('manager.switchAgent.whatHappens') }}</b>
				<ul>
					<li>{{ $t('manager.switchAgent.historyStays') }}</li>
					<li>{{ $t('manager.switchAgent.handoff') }}</li>
					<li>{{ $t('manager.switchAgent.stopped') }}</li>
				</ul>
			</div>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button
					type="button"
					class="btn"
					data-autofocus
					@click="$emit('cancel')"
				>
					{{ $t('manager.trust.cancel') }}
				</button>
				<button
					type="button"
					class="btn primary"
					:disabled="busy || (agent === current && model === currentModel)"
					@click="$emit('submit')"
				>
					{{ $t('manager.switchAgent.submit', { agent, model }) }}
				</button>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { SPAWN_AGENTS } from '../types';
	import Avatar from './Avatar.vue';
	import { dialogKeydown, focusInitial } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerSwitchAgentDialog',
		components: { Avatar },
		props: {
			agent: { type: String, required: true },
			model: { type: String, required: true },
			models: { type: Array as PropType<string[]>, required: true },
			current: { type: String, default: '' },
			currentModel: { type: String, default: '' },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['pick', 'update:model', 'submit', 'cancel'],
		data: () => ({ agents: SPAWN_AGENTS }),
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			cliOnly: (a: string) => a === 'opencode' || a === 'agy',
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('cancel'),
				);
			},
		},
	});
</script>
