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
					{{ a }}
				</button>
			</div>
			<div class="lbl">{{ $t('manager.newSession.machine') }}</div>
			<div class="mods" role="radiogroup">
				<button
					v-for="m in machines"
					:key="m"
					type="button"
					role="radio"
					:aria-checked="form.machine === m"
					:class="['mchip', { on: form.machine === m }]"
					@click="set('machine', m)"
				>
					{{ m }}
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
					<label class="lbl" for="mgr-new-task">{{
						$t('manager.newSession.task')
					}}</label>
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
	import { SPAWN_AGENTS, type SpawnRequest } from '../types';
	import { dialogKeydown, focusInitial } from './trust/trustHelpers';

	export default defineComponent({
		name: 'ManagerNewSessionDialog',
		props: {
			form: { type: Object as PropType<SpawnRequest>, required: true },
			machines: { type: Array as PropType<string[]>, required: true },
			busy: { type: Boolean, default: false },
			error: { type: String, default: '' },
		},
		emits: ['submit', 'cancel'],
		data: () => ({ agents: SPAWN_AGENTS }),
		mounted() {
			focusInitial(this.$refs.dialog as HTMLElement);
		},
		methods: {
			set<K extends keyof SpawnRequest>(key: K, value: SpawnRequest[K]) {
				this.form[key] = value;
			},
			onKey(e: KeyboardEvent) {
				dialogKeydown(this.$refs.dialog as HTMLElement, e, () =>
					this.$emit('cancel'),
				);
			},
		},
	});
</script>
