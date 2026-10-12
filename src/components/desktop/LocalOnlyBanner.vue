<template>
	<div v-if="showFirstRun || warning" class="flex flex-col">
		<div
			v-if="showFirstRun"
			role="status"
			data-testid="local-only-banner"
			data-tour="local.banner"
			class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-sky-300 bg-sky-100 px-4 py-2 text-sm text-sky-900 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-200"
		>
			<span class="min-w-0 flex-1">
				Local only — there is no cloud copy. TMGR backs up your workspace every
				day to
				<strong class="[overflow-wrap:anywhere]" :title="backupsDir">{{
					backupsLabel
				}}</strong
				>.
			</span>
			<button
				type="button"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-sky-200 dark:hover:bg-sky-500/20"
				@click="openBackups"
			>
				Open backups folder
			</button>
			<button
				type="button"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-sky-200 dark:hover:bg-sky-500/20"
				@click="createAccount"
			>
				Create account
			</button>
			<button
				type="button"
				class="rounded-md px-2 py-1 hover:bg-sky-200 dark:hover:bg-sky-500/20"
				aria-label="Dismiss"
				@click="dismissFirstRun"
			>
				<X class="h-4 w-4" />
			</button>
		</div>
		<div
			v-if="warning"
			role="alert"
			data-testid="backup-warning"
			class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-amber-300 bg-amber-100 px-4 py-2 text-sm text-amber-900 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-200"
		>
			<span class="min-w-0 flex-1 break-words">{{ warning.message }}</span>
			<button
				type="button"
				class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-amber-200 disabled:opacity-60 dark:hover:bg-amber-500/20"
				:disabled="backingUp"
				@click="backupNowClick"
			>
				{{ backingUp ? 'Backing up…' : 'Back up now' }}
			</button>
			<button
				type="button"
				class="rounded-md px-2 py-1 hover:bg-amber-200 dark:hover:bg-amber-500/20"
				aria-label="Hide for now"
				@click="hiddenForSession = true"
			>
				<X class="h-4 w-4" />
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { backupNow, backupStatus, revealBackups, type BackupStatus } from '@/local/backups';
	import { activeLocalWorkspace } from '@/local/runtime';
	import { desktopOfferEnv, offerBackups } from '@/tours/desktopOffers';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		backupWarning,
		tildePath,
		bannerDismissed,
		dismissBanner,
	} from '@/utils/localOnly';
	import { X } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const REFRESH_MS = 10 * 60_000;

	export default defineComponent({
		name: 'LocalOnlyBanner',
		components: { X },
		setup() {
			const store = useStore();
			const router = useRouter();
			const statuses = ref<BackupStatus[]>([]);
			const firstRunDismissed = ref(bannerDismissed());
			const hiddenForSession = ref(false);
			const backingUp = ref(false);
			let timer: ReturnType<typeof setInterval> | undefined;

			const localOnly = computed(() => store.getters.isLocalOnly === true);
			const inLocalWorkspace = computed(
				() => localOnly.value || store.getters.currentWorkspace?.is_local === true,
			);
			const watching = computed(
				() => isDesktopApp() && inLocalWorkspace.value,
			);

			const code = computed(
				() =>
					activeLocalWorkspace()?.code ??
					store.getters.currentWorkspace?.code ??
					null,
			);
			const status = computed(
				() => statuses.value.find((s) => s.code === code.value) ?? null,
			);
			const backupsDir = computed(() => status.value?.backups_dir ?? '');
			const backupsLabel = computed(
				() => tildePath(backupsDir.value) || 'the backups folder',
			);

			const refresh = async () => {
				if (!watching.value) return;
				try {
					statuses.value = await backupStatus();
				} catch {
					return;
				}
			};

			onMounted(() => {
				void refresh();
				timer = setInterval(refresh, REFRESH_MS);
			});
			onBeforeUnmount(() => clearInterval(timer));
			watch(watching, () => void refresh());

			const showFirstRun = computed(
				() => isDesktopApp() && localOnly.value && !firstRunDismissed.value,
			);
			const warning = computed(() => {
				if (!watching.value || hiddenForSession.value || !status.value)
					return null;
				return backupWarning(status.value);
			});

			watch(
				warning,
				(value) => {
					if (value) offerBackups(desktopOfferEnv());
				},
				{ immediate: true },
			);

			const dismissFirstRun = () => {
				dismissBanner();
				firstRunDismissed.value = true;
			};
			const openBackups = () => {
				if (status.value) void revealBackups(status.value.code).catch(() => {});
			};
			const createAccount = () => router.push({ name: 'Login' });
			const backupNowClick = async () => {
				if (!status.value || backingUp.value) return;
				backingUp.value = true;
				try {
					await backupNow(status.value.code);
				} catch {
					return;
				} finally {
					backingUp.value = false;
					await refresh();
				}
			};

			return {
				showFirstRun,
				warning,
				backupsDir,
				backupsLabel,
				hiddenForSession,
				backingUp,
				dismissFirstRun,
				openBackups,
				createAccount,
				backupNowClick,
			};
		},
	});
</script>
