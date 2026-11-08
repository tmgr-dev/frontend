<template>
	<SettingsSection :title="$t('settingsPages.alarmPhoneSection.title')">
		<p v-if="loading" class="text-sm text-muted-foreground">
			{{ $t('settingsPages.alarmPhoneSection.loading') }}
		</p>
		<p
			v-else-if="state.kind === 'unconfigured'"
			class="text-sm text-muted-foreground"
		>
			{{ $t('settingsPages.alarmPhoneSection.unconfigured') }}
		</p>
		<p v-else-if="loadError" class="text-sm text-destructive">
			{{ loadError }}
		</p>
		<div v-else-if="state.kind === 'unreadable'" class="flex flex-col gap-3">
			<p class="text-sm text-destructive">
				{{ $t('settingsPages.alarmPhoneSection.unreadable') }}
			</p>
			<div>
				<Button variant="outline" size="sm" :disabled="busy" @click="remove">
					{{ $t('common.actions.remove') }}
				</Button>
			</div>
		</div>
		<div v-else class="flex flex-col gap-3">
			<p class="text-sm text-muted-foreground">
				{{ $t('settingsPages.alarmPhoneSection.intro') }}
			</p>

			<div
				v-if="state.kind === 'set' && !editing"
				class="flex flex-wrap items-center justify-between gap-3 rounded border border-border p-2"
			>
				<div class="flex min-w-0 items-center gap-2">
					<span class="truncate font-mono text-sm">{{
						state.phone.masked
					}}</span>
					<span
						:class="[
							'shrink-0 rounded px-1.5 py-0.5 text-xs font-medium',
							state.phone.verified
								? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400'
								: 'bg-amber-500/15 text-amber-700 dark:text-amber-400',
						]"
					>
						{{
							state.phone.verified
								? $t('settingsPages.alarmPhoneSection.verified')
								: $t('settingsPages.alarmPhoneSection.notVerified')
						}}
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
						{{ $t('settingsPages.alarmPhoneSection.testCall') }}
					</Button>
					<Button
						variant="outline"
						size="sm"
						:disabled="busy"
						@click="startEdit"
					>
						{{ $t('settingsPages.alarmPhoneSection.change') }}
					</Button>
					<Button variant="outline" size="sm" :disabled="busy" @click="remove">
						{{ $t('common.actions.remove') }}
					</Button>
				</div>
			</div>

			<div
				v-if="callerNumber && !editing"
				class="flex flex-col gap-2 rounded border border-border bg-muted/40 p-3"
			>
				<p class="text-sm">
					<i18n-t
						keypath="settingsPages.alarmPhoneSection.callsFrom"
						tag="span"
					>
						<template #number>
							<span class="font-mono font-medium">{{ callerNumber }}</span>
						</template>
					</i18n-t>
				</p>
				<div class="flex flex-wrap gap-2">
					<Button variant="outline" size="sm" @click="copyCaller">{{
						$t('common.actions.copy')
					}}</Button>
					<Button
						variant="outline"
						size="sm"
						:disabled="busy"
						@click="addContact"
					>
						{{ $t('settingsPages.alarmPhoneSection.addContact') }}
					</Button>
				</div>
				<ul class="list-disc pl-5 text-xs text-muted-foreground">
					<li>{{ $t('settingsPages.alarmPhoneSection.tipFavorites') }}</li>
					<li>
						{{ $t('settingsPages.alarmPhoneSection.tipIphone') }}
					</li>
					<li>
						{{ $t('settingsPages.alarmPhoneSection.tipAndroid') }}
					</li>
				</ul>
			</div>

			<form
				v-if="state.kind === 'set' && !state.phone.verified && !editing"
				class="flex flex-col gap-3"
				@submit.prevent="verify"
			>
				<label class="flex flex-col gap-1">
					<span class="text-sm font-medium">{{
						$t('settingsPages.alarmPhoneSection.verificationCode')
					}}</span>
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
					<Button type="submit" :disabled="busy || !codeValid">{{
						$t('settingsPages.alarmPhoneSection.verify')
					}}</Button>
					<Button
						type="button"
						variant="outline"
						:disabled="busy || cooldown > 0 || !!lockout"
						@click="resend"
					>
						{{
							cooldown > 0
								? $t('settingsPages.alarmPhoneSection.resendIn', {
										seconds: cooldown,
									})
								: $t('settingsPages.alarmPhoneSection.resend')
						}}
					</Button>
				</div>
				<p v-if="lockout" class="text-xs text-muted-foreground">
					{{
						$t('settingsPages.alarmPhoneSection.tryAgainIn', { time: lockout })
					}}
				</p>
			</form>

			<form
				v-else-if="state.kind !== 'set' || editing"
				class="flex flex-col gap-3"
				@submit.prevent="submit"
			>
				<label class="flex flex-col gap-1">
					<span class="text-sm font-medium">{{
						$t('settingsPages.alarmPhoneSection.phoneLabel')
					}}</span>
					<Input
						:model-value="phone"
						type="tel"
						autocomplete="off"
						placeholder="+15550100123"
						@update:model-value="(v) => (phone = String(v))"
					/>
				</label>
				<p v-if="phone && !valid" class="text-xs text-destructive">
					{{ $t('settingsPages.alarmPhoneSection.invalidFormat') }}
				</p>
				<div class="flex gap-2">
					<Button type="submit" :disabled="busy || !valid">{{
						$t('common.actions.save')
					}}</Button>
					<Button
						v-if="state.kind === 'set'"
						type="button"
						variant="outline"
						:disabled="busy"
						@click="cancelEdit"
					>
						{{ $t('common.actions.cancel') }}
					</Button>
				</div>
			</form>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import {
		alarmCallerNumber,
		alarmPhoneError,
		downloadAlarmContact,
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
	import { t } from '@/i18n';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';

	const COOLDOWN_SECONDS = 60;

	const messageFor = (error: AlarmPhoneError): string => {
		switch (error.kind) {
			case 'call_failed':
				return t('settingsPages.alarmPhoneSection.errors.callFailed');
			case 'unconfigured':
				return t('settingsPages.alarmPhoneSection.errors.unconfigured');
			case 'invalid':
				return (
					error.message ??
					t('settingsPages.alarmPhoneSection.errors.invalidPhone')
				);
			case 'rate_limited':
				return error.retryAfter && error.retryAfter > COOLDOWN_SECONDS
					? t('settingsPages.alarmPhoneSection.errors.tryAgainIn', {
							time: formatRetryAfter(error.retryAfter),
						})
					: t('settingsPages.alarmPhoneSection.errors.tooMany');
			case 'reset':
				return t('settingsPages.alarmPhoneSection.errors.reset');
			case 'conflict':
				return t('settingsPages.alarmPhoneSection.errors.conflict');
			default:
				return t('settingsPages.alarmPhoneSection.errors.generic');
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
			const callerNumber = computed(() => alarmCallerNumber(state.value));
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
					loadError.value = t('settingsPages.alarmPhoneSection.loadFailed');
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
					await reload();
				} catch (error) {
					const detail = alarmPhoneError(error);
					if (detail.kind === 'conflict') {
						await reload();
						toast({
							title: t('settingsPages.alarmPhoneSection.numberChanged'),
							variant: 'destructive',
						});
					} else if (detail.kind === 'wrong_code') {
						codeError.value =
							detail.attemptsLeft === undefined
								? t('settingsPages.alarmPhoneSection.wrongCode')
								: t('settingsPages.alarmPhoneSection.wrongCodeLeft', {
										count: detail.attemptsLeft,
									});
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
					toast({ title: t('settingsPages.alarmPhoneSection.calling') });
				} catch (error) {
					const detail = alarmPhoneError(error);
					if (detail.kind === 'unconfigured') {
						toast({
							title: t('settingsPages.alarmPhoneSection.callsUnconfigured'),
							variant: 'destructive',
						});
					} else if (detail.kind === 'conflict') {
						toast({
							title: t('settingsPages.alarmPhoneSection.verifyFirst'),
							variant: 'destructive',
						});
					} else {
						fail(error);
					}
				} finally {
					busy.value = false;
				}
			};

			const copyCaller = async () => {
				if (!callerNumber.value) return;
				try {
					await navigator.clipboard.writeText(callerNumber.value);
					toast({ title: t('common.actions.copied') });
				} catch {
					toast({
						title: t('settingsPages.alarmPhoneSection.copyFailed'),
						variant: 'destructive',
					});
				}
			};

			const addContact = async () => {
				busy.value = true;
				try {
					await downloadAlarmContact();
				} catch {
					toast({
						title: t('settingsPages.alarmPhoneSection.contactFailed'),
						variant: 'destructive',
					});
				} finally {
					busy.value = false;
				}
			};

			const remove = async () => {
				if (!window.confirm(t('settingsPages.alarmPhoneSection.confirmRemove')))
					return;
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
				callerNumber,
				copyCaller,
				addContact,
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
