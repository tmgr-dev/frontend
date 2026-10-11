<template>
	<SettingsSection
		title="Profile"
		description="This name is shown in your local workspaces."
	>
		<form class="flex max-w-xl flex-col gap-1.5" @submit.prevent="save">
			<Label for="local-profile-name">Name</Label>
			<div class="flex flex-wrap items-center gap-2">
				<Input
					id="local-profile-name"
					v-model="name"
					class="min-w-[220px] flex-1"
					autocomplete="name"
					:disabled="saving"
				/>
				<Button type="submit" :disabled="saving || !changed">
					{{ saving ? 'Saving…' : 'Save' }}
				</Button>
			</div>
			<p v-if="error" role="alert" class="text-sm text-destructive">
				{{ error }}
			</p>
			<p v-else-if="saved" role="status" class="text-sm text-ink-subtle">
				Saved.
			</p>
		</form>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { updateLocalProfile } from '@/utils/localProfile';
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
					error.value = e?.message || String(e) || 'Could not save the name.';
				} finally {
					saving.value = false;
				}
			};

			return { name, saving, saved, error, changed, save };
		},
	});
</script>
