<template>
	<SettingsSection
		:title="$t('desktop.localProfile.title')"
		:description="$t('desktop.localProfile.description')"
	>
		<form class="flex max-w-xl flex-col gap-1.5" @submit.prevent="save">
			<Label for="local-profile-name">{{
				$t('desktop.localProfile.name')
			}}</Label>
			<div class="flex flex-wrap items-center gap-2">
				<Input
					id="local-profile-name"
					v-model="name"
					class="min-w-[220px] flex-1"
					autocomplete="name"
					:disabled="saving"
				/>
				<Button type="submit" :disabled="saving || !changed">
					{{ saving ? $t('common.actions.saving') : $t('common.actions.save') }}
				</Button>
			</div>
			<p v-if="error" role="alert" class="text-sm text-destructive">
				{{ error }}
			</p>
			<p v-else-if="saved" role="status" class="text-sm text-ink-subtle">
				{{ $t('desktop.localProfile.saved') }}
			</p>
		</form>
		<div
			v-if="linkedUserId"
			class="mt-4 flex max-w-xl flex-wrap items-center gap-2"
		>
			<p class="flex-1 text-sm text-ink-subtle">
				{{
					linkedToCurrent
						? $t('desktop.localProfile.linkedToYou')
						: $t('desktop.localProfile.linkedToOther')
				}}
			</p>
			<Button
				type="button"
				variant="outline"
				:disabled="unlinking"
				@click="unlink"
			>
				{{
					unlinking
						? $t('desktop.localProfile.unlinking')
						: $t('desktop.localProfile.unlink')
				}}
			</Button>
			<p
				v-if="unlinkError"
				role="alert"
				class="w-full text-sm text-destructive"
			>
				{{ unlinkError }}
			</p>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { t } from '@/i18n';
	import { unlinkLocalProfile, updateLocalProfile } from '@/utils/localProfile';
	import { computed, defineComponent, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'LocalProfileSettings',
		components: { SettingsSection, Button, Input, Label },
		setup() {
			const store = useStore();
			const current = computed(
				() => store.state.localProfile?.name ?? store.state.user?.name ?? '',
			);
			const name = ref(current.value);
			const saving = ref(false);
			const saved = ref(false);
			const error = ref('');

			watch(current, (value) => {
				if (!saving.value) name.value = value;
			});

			const changed = computed(() => name.value.trim() !== current.value);

			const save = async () => {
				if (saving.value || !changed.value) return;
				saving.value = true;
				saved.value = false;
				error.value = '';
				try {
					await updateLocalProfile(name.value.trim());
					await store.dispatch('refreshLocalProfile');
					saved.value = true;
				} catch (e: any) {
					error.value =
						e?.message || String(e) || t('desktop.localProfile.saveFailed');
				} finally {
					saving.value = false;
				}
			};

			const linkedUserId = computed(
				() => store.state.localProfile?.linked_user_id ?? null,
			);
			const linkedToCurrent = computed(
				() => linkedUserId.value === (store.state.user?.id ?? null),
			);
			const unlinking = ref(false);
			const unlinkError = ref('');

			const unlink = async () => {
				if (unlinking.value) return;
				unlinking.value = true;
				unlinkError.value = '';
				try {
					await unlinkLocalProfile();
					await store.dispatch('refreshLocalProfile');
				} catch (e: any) {
					unlinkError.value =
						e?.message || String(e) || t('desktop.localProfile.unlinkFailed');
				} finally {
					unlinking.value = false;
				}
			};

			return {
				name,
				saving,
				saved,
				error,
				changed,
				save,
				linkedUserId,
				linkedToCurrent,
				unlinking,
				unlinkError,
				unlink,
			};
		},
	});
</script>
