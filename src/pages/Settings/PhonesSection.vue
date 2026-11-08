<template>
	<SettingsSection v-if="available" title="Phones">
		<div class="flex flex-col gap-3">
			<p class="text-sm text-muted-foreground">
				Answer Decisions cards from the TMGR iPhone app. Pairing needs this Mac
				to be on.
			</p>
			<label class="flex max-w-xs flex-col gap-1">
				<span class="text-sm font-medium">Name of this Mac</span>
				<Input
					:model-value="desktopName"
					maxlength="40"
					@update:model-value="(v) => (desktopName = String(v))"
				/>
			</label>
			<p v-if="error" class="text-sm text-destructive" role="alert">
				{{ error }}
			</p>
			<div class="flex flex-wrap gap-2">
				<Button type="button" size="sm" :disabled="busy" @click="pair">
					Pair an iPhone
				</Button>
				<Button
					type="button"
					variant="outline"
					size="sm"
					:disabled="busy"
					@click="manage"
				>
					Manage phones
				</Button>
			</div>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import { createPhoneMailbox } from '@/actions/tmgr/phoneRelay';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { phonesBeginPairing, phonesOpenWindow } from '@/phones/api';
	import { pairIphone } from '@/phones/pair';
	import { DEFAULT_DESKTOP_NAME, phonesAvailable } from '@/phones/view';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, defineComponent, ref } from 'vue';

	export default defineComponent({
		name: 'PhonesSection',
		components: { SettingsSection, Button, Input },
		setup() {
			const desktopName = ref(DEFAULT_DESKTOP_NAME);
			const error = ref('');
			const busy = ref(false);
			const available = computed(() =>
				phonesAvailable(isDesktopApp(), store.getters.isLocalOnly),
			);

			const pair = async () => {
				busy.value = true;
				error.value = '';
				const result = await pairIphone(
					{
						createMailbox: createPhoneMailbox,
						begin: phonesBeginPairing,
						apiBase: import.meta.env.VITE_API_BASE_URL,
						userId: store.state.user?.id,
					},
					desktopName.value,
				);
				if (!result.ok) error.value = result.error;
				busy.value = false;
			};

			const manage = async () => {
				error.value = '';
				try {
					await phonesOpenWindow();
				} catch (e) {
					error.value = typeof e === 'string' ? e : 'Could not open the window';
				}
			};

			return { desktopName, error, busy, available, pair, manage };
		},
	});
</script>
