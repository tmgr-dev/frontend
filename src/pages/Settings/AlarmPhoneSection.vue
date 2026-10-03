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
		<div v-else-if="state.kind === 'unreadable'" class="flex flex-col gap-3">
			<p class="text-sm text-destructive">
				The stored number can't be read. Remove it and enter it again.
			</p>
			<div>
				<Button variant="outline" size="sm" :disabled="busy" @click="remove">
					Remove
				</Button>
			</div>
		</div>
		<div v-else class="flex flex-col gap-3">
			<p class="text-sm text-muted-foreground">
				Used only for critical alarms when push is not acknowledged. Stored
				encrypted. We'll text you a code to verify it.
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
					<Button
						v-if="state.phone.verified"
						variant="outline"
						size="sm"
						:disabled="busy"
						@click="testCall"
					>
						Test call
					</Button>
					<Button variant="outline" size="sm" :disabled="busy" @click="startEdit">
						Change
					</Button>
					<Button variant="outline" size="sm" :disabled="busy" @click="remove">
						Remove
					</Button>
				</div>
			</div>

			<form
				v-if="state.kind === 'set' && !state.phone.verified && !editing"
				class="flex flex-col gap-3"
				@submit.prevent="verify"
			>
				<label class="flex flex-col gap-1">
					<span class="text-sm font-medium">Verification code</span>
					<Input
						:model-value="code"
						inputmode="numeric"
						autocomplete="one-time-code"
						maxlength="10"
						placeholder="123456"
						@update:model-value="(v) => (code = String(v).replace(/\D/g, ''))"
					/>
				</label>
				<p v-if="codeError" class="text-xs text-destructive">
					{{ codeError }}
				</p>
				<div class="flex gap-2">
					<Button type="submit" :disabled="busy || !codeValid">Verify</Button>
					<Button
						type="button"
						variant="outline"
						:disabled="busy || cooldown > 0 || !!lockout"
						@click="resend"
					>
						{{ cooldown > 0 ? `Resend in ${cooldown}s` : 'Resend code' }}
					</Button>
				</div>
				<p v-if="lockout" class="text-xs text-muted-foreground">
					Try again in {{ lockout }}
				</p>
			</form>

			<form v-else-if="state.kind !== 'set' || editing" class="flex flex-col gap-3" @submit.prevent="submit">
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
		alarmPhoneError,
		fetchAlarmPhone,
		formatRetryAfter,
		isValidAlarmCode,
		isValidAlarmPhone,
		removeAlarmPhone,
		resendAlarmCode,
		saveAlarmPhone,
		startAlarmTestCall,
		verifyAlarmPhone,
		type AlarmPhoneError,
		type AlarmPhoneState,
	} from '@/actions/tmgr/alarmPhone';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { toast } from '@/components/ui/toast';
	import { computed, defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';

	const COOLDOWN_SECONDS = 60;
	const RESET_MESSAGE = 'Too many wrong codes. Enter the number again.';

	const messageFor = (error: AlarmPhoneError): string => {
		switch (error.kind) {
			case 'call_failed':
				return 'Could not place the call';
			case 'unconfigured':
				return 'Alarm phone is not configured on this server';
			case 'invalid':
				return error.message ?? 'Invalid phone number';
			case 'rate_limited':
				return error.retryAfter && error.retryAfter > COOLDOWN_SECONDS
					? `Try again in ${formatRetryAfter(error.retryAfter)}`
					: 'Too many attempts, try again later';
			case 'reset':
				return RESET_MESSAGE;
			case 'conflict':
				return 'Nothing to do for this number right now';
			default:
				return 'Something went wrong';
		}
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
			const phone = ref('');
			const code = ref('');
			const codeError = ref<string | null>(null);
			const cooldown = ref(0);
			const valid = computed(() => isValidAlarmPhone(phone.value));
			const codeValid = computed(() => isValidAlarmCode(code.value));
			const lockout = ref('');
			let timer: ReturnType<typeof setInterval> | null = null;
			let lockoutTimer: ReturnType<typeof setTimeout> | null = null;

			const stopTimer = () => {
				if (timer) clearInterval(timer);
				timer = null;
				if (lockoutTimer) clearTimeout(lockoutTimer);
				lockoutTimer = null;
				lockout.value = '';
			};

			const startLockout = (seconds: number) => {
				stopTimer();
				cooldown.value = 0;
				lockout.value = formatRetryAfter(seconds);
				lockoutTimer = setTimeout(stopTimer, seconds * 1000);
			};

			const startCooldown = (seconds = COOLDOWN_SECONDS) => {
				stopTimer();
				cooldown.value = seconds;
				if (seconds <= 0) return;
				timer = setInterval(() => {
					cooldown.value -= 1;
					if (cooldown.value <= 0) stopTimer();
				}, 1000);
			};

			const clearToEmpty = () => {
				state.value = { kind: 'unset' };
				editing.value = false;
				phone.value = '';
				code.value = '';
				codeError.value = null;
				stopTimer();
				cooldown.value = 0;
			};

			const fail = (error: unknown) => {
				const detail = alarmPhoneError(error);
				if (detail.kind === 'unconfigured') {
					state.value = { kind: 'unconfigured' };
					return;
				}
				if (detail.kind === 'reset') clearToEmpty();
				if (detail.kind === 'rate_limited') {
					if (detail.retryAfter && detail.retryAfter > COOLDOWN_SECONDS) {
						startLockout(detail.retryAfter);
					} else {
						startCooldown(detail.retryAfter ?? COOLDOWN_SECONDS);
					}
				}
				toast({ title: messageFor(detail), variant: 'destructive' });
			};

			const reload = async () => {
				try {
					state.value = await fetchAlarmPhone();
				} catch {
					return;
				}
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
					state.value = {
						kind: 'set',
						phone: { masked: saved.masked, verified: saved.verified },
					};
					phone.value = '';
					code.value = '';
					codeError.value = null;
					editing.value = false;
					if (saved.codeSent) startCooldown();
				} catch (error) {
					fail(error);
					if (alarmPhoneError(error).kind !== 'invalid') await reload();
				} finally {
					busy.value = false;
				}
			};

			const verify = async () => {
				if (!codeValid.value) return;
				busy.value = true;
				codeError.value = null;
				try {
					const verified = await verifyAlarmPhone(code.value);
					state.value = {
						kind: 'set',
						phone: { masked: verified.masked, verified: true },
					};
					code.value = '';
					stopTimer();
					cooldown.value = 0;
				} catch (error) {
					const detail = alarmPhoneError(error);
					if (detail.kind === 'conflict') {
						await reload();
						toast({ title: 'This number changed, please re-check', variant: 'destructive' });
					} else if (detail.kind === 'wrong_code') {
						codeError.value =
							detail.attemptsLeft === undefined
								? 'Wrong code.'
								: `Wrong code. Attempts left: ${detail.attemptsLeft}.`;
					} else {
						fail(error);
					}
				} finally {
					busy.value = false;
				}
			};

			const resend = async () => {
				busy.value = true;
				codeError.value = null;
				try {
					await resendAlarmCode();
					startCooldown();
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
					toast({ title: 'Calling now.' });
				} catch (error) {
					const detail = alarmPhoneError(error);
					if (detail.kind === 'unconfigured') {
						toast({ title: 'Calls are not configured on this server', variant: 'destructive' });
					} else if (detail.kind === 'conflict') {
						toast({ title: 'Verify the number first', variant: 'destructive' });
					} else {
						fail(error);
					}
				} finally {
					busy.value = false;
				}
			};

			const remove = async () => {
				if (!window.confirm('Remove the alarm phone?')) return;
				busy.value = true;
				try {
					await removeAlarmPhone();
					clearToEmpty();
				} catch (error) {
					fail(error);
				} finally {
					busy.value = false;
				}
			};

			onMounted(load);
			onBeforeUnmount(stopTimer);

			return {
				state,
				loading,
				loadError,
				busy,
				editing,
				phone,
				code,
				codeError,
				cooldown,
				lockout,
				valid,
				codeValid,
				startEdit,
				cancelEdit,
				submit,
				verify,
				resend,
				testCall,
				remove,
			};
		},
	});
</script>
