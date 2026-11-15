<script setup lang="ts">
	import { switchLanguage } from '@/components/general/switchLanguage';
	import {
		DropdownMenuRadioGroup,
		DropdownMenuRadioItem,
		DropdownMenuSub,
		DropdownMenuSubContent,
		DropdownMenuSubTrigger,
	} from '@/components/ui/dropdown-menu';
	import { useToast } from '@/components/ui/toast';
	import { i18n, LOCALE_NAMES, SUPPORTED_LOCALES, t } from '@/i18n';
	import { Languages } from 'lucide-vue-next';
	import { computed, ref } from 'vue';

	const toaster = useToast();
	const saving = ref(false);
	const current = computed(() => i18n.global.locale.value);

	const onSelect = async (value: unknown) => {
		if (typeof value !== 'string' || saving.value) return;
		saving.value = true;
		const ok = await switchLanguage(value, current.value);
		saving.value = false;
		if (!ok) {
			toaster.toast({
				title: t('nav.sidebar.languageSaveFailed'),
				variant: 'destructive',
			});
		}
	};
</script>

<template>
	<DropdownMenuSub>
		<DropdownMenuSubTrigger data-testid="language-menu-trigger">
			<Languages /> {{ $t('nav.sidebar.language') }}
		</DropdownMenuSubTrigger>
		<DropdownMenuSubContent class="min-w-44">
			<DropdownMenuRadioGroup
				:model-value="current"
				@update:model-value="onSelect"
			>
				<DropdownMenuRadioItem
					v-for="locale in SUPPORTED_LOCALES"
					:key="locale"
					:value="locale"
					class="cursor-pointer"
					:data-testid="`language-option-${locale}`"
				>
					{{ LOCALE_NAMES[locale] }}
				</DropdownMenuRadioItem>
			</DropdownMenuRadioGroup>
		</DropdownMenuSubContent>
	</DropdownMenuSub>
</template>
