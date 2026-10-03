<template>
	<SettingsSection title="Alarm phone">
		<p v-if="loading" class="text-sm text-muted-foreground">
			Loading alarm phone…
		</p>
		<p v-else-if="state.kind === 'unconfigured'" class="text-sm text-muted-foreground">
			Alarm phone is not configured on this server.
		</p>
		<p v-else-if="loadError" class="text-sm text-destructive">
			{{ loadError }}
		</p>
		<div v-else class="flex flex-col gap-3">
			<p class="text-sm text-muted-foreground">
				Used only for critical alarms when push is not acknowledged. Stored
				encrypted. Answer the test call and press 1 to verify.
			</p>

			<div
				v-if="state.kind === 'set' && !editing"
				class="flex flex-wrap items-center justify-between gap-3 rounded border border-border p-2"
			>
				<div class="flex min-w-0 items-center gap-2">
					<span class="truncate font-mono text-sm">{{ state.phone.masked }}</span>
					<span
						:class="[
							'shrink-0 rounded px-1.5 py-0.5 text-xs font-medium',
							state.phone.verified
								? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400'
								: 'bg-amber-500/15 text-amber-700 dark:text-amber-400',
						]"
					>
						{{ state.phone.verified ? 'Verified' : 'Not verified' }}
					</span>
				</div>
				<div class="flex flex-wrap gap-2">
					<Button variant="outline" size="sm" :disabled="busy" @click="testCall">
						Test call
					</Button>
					<Button
						v-if="calledOnce"
						variant="outline"
						size="sm"
						:disabled="busy"
						@click="load"
					>
						Refresh
					</Button>
					<Button variant="outline" size="sm" :disabled="busy" @click="startEdit">
						Change
					</Button>
					<Button variant="outline" size="sm" :disabled="busy" @click="remove">
						Remove
					</Button>
				</div>
			</div>

			<form v-else class="flex flex-col gap-3" @submit.prevent="submit">
				<label class="flex flex-col gap-1">
					<span class="text-sm font-medium">Phone number (E.164)</span>
					<Input
						:model-value="phone"
						type="tel"
						autocomplete="off"
						placeholder="+15550100123"
						@update:model-value="(v) => (phone = String(v))"
					/>
				</label>
				<p v-if="phone && !valid" class="text-xs text-destructive">
					Use international format, e.g. +15550100123.
				</p>
				<div class="flex gap-2">
					<Button type="submit" :disabled="busy || !valid">Save</Button>
					<Button
						v-if="state.kind === 'set'"
						type="button"
						variant="outline"
						:disabled="busy"
						@click="cancelEdit"
					>
						Cancel
					</Button>
				</div>
			</form>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import {
		alarmPhoneFailure,
		fetchAlarmPhone,
		isValidAlarmPhone,
		removeAlarmPhone,
		saveAlarmPhone,
		startAlarmTestCall,
		type AlarmPhoneFailure,
		type AlarmPhoneState,
	} from '@/actions/tmgr/alarmPhone';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { toast } from '@/components/ui/toast';
	import { computed, defineComponent, onMounted, ref } from 'vue';

	const MESSAGES: Record<AlarmPhoneFailure, string> = {
		unconfigured: 'Alarm phone is not configured on this server',
		invalid: 'Invalid phone number',
		rate_limited: 'Too many test calls, try again later',
		failed: 'Something went wrong',
	};

	export default defineComponent({
		name: 'AlarmPhoneSection',
		components: { SettingsSection, Button, Input },
		setup() {
			const state = ref<AlarmPhoneState>({ kind: 'unset' });
			const loading = ref(true);
			const loadError = ref<string | null>(null);
			const busy = ref(false);
			const editing = ref(false);
			const calledOnce = ref(false);
			const phone = ref('');
			const valid = computed(() => isValidAlarmPhone(phone.value));

			const fail = (error: unknown) => {
				const failure = alarmPhoneFailure(error);
				if (failure === 'unconfigured') state.value = { kind: 'unconfigured' };
				else toast({ title: MESSAGES[failure], variant: 'destructive' });
			};

			const load = async () => {
				busy.value = true;
				loadError.value = null;
				try {
					state.value = await fetchAlarmPhone();
				} catch {
					loadError.value = 'Could not load alarm phone.';
				} finally {
					loading.value = false;
					busy.value = false;
				}
			};

			const startEdit = () => {
				phone.value = '';
				editing.value = true;
			};

			const cancelEdit = () => {
				phone.value = '';
				editing.value = false;
			};

			const submit = async () => {
				if (!valid.value) return;
				busy.value = true;
				try {
					const saved = await saveAlarmPhone(phone.value);
					state.value = { kind: 'set', phone: saved };
					phone.value = '';
					editing.value = false;
					calledOnce.value = false;
				} catch (error) {
					fail(error);
				} finally {
					busy.value = false;
				}
			};

			const testCall = async () => {
				busy.value = true;
				try {
					await startAlarmTestCall();
					calledOnce.value = true;
					toast({ title: 'Calling now. Answer and press 1 to verify.' });
				} catch (error) {
					fail(error);
				} finally {
					busy.value = false;
				}
			};

			const remove = async () => {
				if (!window.confirm('Remove the alarm phone?')) return;
				busy.value = true;
				try {
					await removeAlarmPhone();
					state.value = { kind: 'unset' };
					calledOnce.value = false;
					editing.value = false;
				} catch (error) {
					fail(error);
				} finally {
					busy.value = false;
				}
			};

			onMounted(load);

			return {
				state,
				loading,
				loadError,
				busy,
				editing,
				calledOnce,
				phone,
				valid,
				load,
				startEdit,
				cancelEdit,
				submit,
				testCall,
				remove,
			};
		},
	});
</script>
