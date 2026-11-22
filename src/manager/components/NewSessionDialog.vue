<template>
	<div class="trust-overlay">
		<form
			ref="dialog"
			class="trust-dialog wide nsf"
			role="dialog"
			aria-modal="true"
			aria-labelledby="mgr-new-title"
			@keydown="onKey"
			@submit.prevent="$emit('submit')"
		>
			<h2 id="mgr-new-title">{{ $t('manager.newSession.title') }}</h2>
			<div class="lbl">{{ $t('manager.newSession.machine') }}</div>
			<div class="seg" role="radiogroup">
				<button
					v-for="m in options"
					:key="m.id"
					type="button"
					role="radio"
					:aria-checked="form.machine === m.id"
					:class="['mchip', { on: form.machine === m.id }]"
					@click="set('machine', m.id)"
				>
					{{ m.label
					}}<template v-if="m.linked">
						&middot; {{ $t('manager.newSession.linked') }}</template
					>
				</button>
			</div>
			<div class="lbl">{{ $t('manager.newSession.agent') }}</div>
			<div class="mods" role="radiogroup">
				<button
					v-for="a in agents"
					:key="a"
					type="button"
					role="radio"
					:aria-checked="form.agent === a"
					:class="['mchip', { on: form.agent === a }]"
					@click="set('agent', a)"
				>
					{{ agentLabels[a] }}
				</button>
			</div>
			<div class="lbl">{{ $t('manager.newSession.safety') }}</div>
			<div class="safe">
				<button
					v-for="s in safety"
					:key="s.key"
					type="button"
					role="switch"
					:aria-checked="!!form[s.key]"
					:class="['safe-row', { on: !!form[s.key] }]"
					@click="set(s.key, !form[s.key])"
				>
					<span class="tgl" aria-hidden="true"></span>
					<span class="safe-txt"
						><b>{{ $t(`manager.newSession.${s.key}`) }}</b
						><span>{{ $t(`manager.newSession.${s.key}Hint`) }}</span></span
					>
				</button>
			</div>
			<label class="lbl" for="mgr-new-folder">{{
				$t('manager.newSession.folder')
			}}</label>
			<input
				id="mgr-new-folder"
				class="fld mono"
				type="text"
				:value="form.folder"
				placeholder="~/projects/app"
				data-autofocus
				@input="set('folder', ($event.target as HTMLInputElement).value)"
			/>
			<div class="two">
				<div>
					<label class="lbl" for="mgr-new-task"
						>{{ $t('manager.newSession.task') }}
						<i>{{ $t('manager.newSession.optional') }}</i></label
					>
					<input
						id="mgr-new-task"
						class="fld"
						type="text"
						:value="form.task"
						placeholder="TM-1048"
						@input="set('task', ($event.target as HTMLInputElement).value)"
					/>
				</div>
				<div>
					<label class="lbl" for="mgr-new-name">{{
						$t('manager.newSession.name')
					}}</label>
					<input
						id="mgr-new-name"
						class="fld"
						type="text"
						:value="form.name"
						@input="set('name', ($event.target as HTMLInputElement).value)"
					/>
				</div>
			</div>
			<div class="lbl">{{ $t('manager.newSession.template') }}</div>
			<div class="mods">
				<button
					v-for="t in templates"
					:key="t.id"
					type="button"
					:class="['mchip', { on: template === t.id }]"
					:aria-pressed="template === t.id"
					@click="pick(t.id)"
				>
					{{ $t(`manager.newSession.templates.${t.id}`) }}
				</button>
			</div>
			<label class="lbl" for="mgr-new-prompt">{{
				$t('manager.newSession.prompt')
			}}</label>
			<textarea
				id="mgr-new-prompt"
				class="fld ta"
				rows="3"
				:value="form.prompt"
				:placeholder="$t('manager.newSession.promptPlaceholder')"
				@input="set('prompt', ($event.target as HTMLTextAreaElement).value)"
			></textarea>
			<p v-if="error" class="mgr-error" role="alert">{{ error }}</p>
			<div class="trust-actions">
				<button type="button" class="btn" @click="$emit('cancel')">
					{{ $t('manager.trust.cancel') }}
				</button>
				<button
					type="submit"
					class="btn primary"
					:disabled="busy || !form.machine || !form.folder.trim()"
				>
					{{ $t('manager.newSession.start') }}
				</button>
			</div>
		</form>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import {
		AGENT_LABELS,
		SESSION_TEMPLATES,
		applyTemplate,
		machineOptions,
	} from '../newSession';
	import {
		SPAWN_AGENTS,
		type ManagerMachine,
		type SpawnRequest,
	} from '../types';
	import { dialogKeydown, focusInitial } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerNewSessionDialog',
		props: {
			form: { type: Object as PropType<SpawnRequest>, required: true },
			machines: { type: Array as PropType<string[]>, required: true },
			machineInfo: {
				type: Array as PropType<ManagerMachine[]>,
				default: () => [],
			},
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['submit', 'cancel'],
		data: () => ({
			agents: SPAWN_AGENTS,
			agentLabels: AGENT_LABELS,
			templates: SESSION_TEMPLATES,
			template: 'blank',
			safety: [{ key: 'noSandbox' as const }, { key: 'bypass' as const }],
		}),
		computed: {
			options() {
				return machineOptions(this.machines, this.machineInfo);
			},
		},
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			set<K extends keyof SpawnRequest>(key: K, value: SpawnRequest[K]) {
				this.form[key] = value;
			},
			pick(id: string) {
				this.template = id;
				this.form.prompt = applyTemplate(this.form, id).prompt;
			},
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('cancel'),
				);
			},
		},
	});
</script>
