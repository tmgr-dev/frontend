<template>
	<div class="flex flex-col gap-4">
		<i18n-t
			keypath="desktop.gitTrack.intro"
			tag="p"
			class="text-sm text-muted-foreground"
		>
			<template #example>
				<span class="font-mono">feature/TM-157-login</span>
			</template>
		</i18n-t>
		<fieldset class="flex flex-col gap-1">
			<legend class="mb-2 text-sm font-medium">
				{{ $t('desktop.gitTrack.mode') }}
			</legend>
			<label
				v-for="option in modes"
				:key="option.value"
				class="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-muted/50 dark:hover:bg-muted/30"
			>
				<input
					type="radio"
					name="git-track-mode"
					class="mt-1 accent-primary"
					:value="option.value"
					:checked="mode === option.value"
					@change="setMode(option.value)"
				/>
				<span class="flex flex-col">
					<span class="text-sm font-medium">{{ $t(option.labelKey) }}</span>
					<span class="text-xs text-muted-foreground">{{
						$t(option.hintKey)
					}}</span>
				</span>
			</label>
		</fieldset>
		<div class="flex flex-col gap-2">
			<div class="flex items-center justify-between gap-3">
				<span class="text-sm font-medium">{{
					$t('desktop.gitTrack.watchedFolders')
				}}</span>
				<Button
					variant="outline"
					size="sm"
					:disabled="folders.length >= maxFolders || adding"
					@click="addFolder"
				>
					<Plus class="mr-1 h-4 w-4" />
					{{ $t('desktop.gitTrack.addFolder') }}
				</Button>
			</div>
			<p v-if="!folders.length" class="text-xs text-muted-foreground">
				{{ $t('desktop.gitTrack.noFolders') }}
			</p>
			<ul v-else class="flex flex-col gap-1">
				<li
					v-for="folder in folders"
					:key="folder"
					class="flex items-center gap-2 rounded-md px-2 py-1 hover:bg-muted/50 dark:hover:bg-muted/30"
				>
					<span class="min-w-0 flex-1 font-mono text-xs break-all">
						{{ folder }}
					</span>
					<Button
						variant="ghost"
						size="sm"
						:title="$t('desktop.gitTrack.removeFolder', { folder })"
						@click="removeFolder(folder)"
					>
						<X class="h-4 w-4" />
					</Button>
				</li>
			</ul>
			<p v-if="error" class="text-xs text-destructive">{{ error }}</p>
		</div>
	</div>
</template>

<script>
	import { Button } from '@/components/ui/button';
	import {
		loadGitTrackFolders,
		loadGitTrackMode,
		MAX_GIT_TRACK_FOLDERS,
		saveGitTrackFolders,
		saveGitTrackMode,
		syncGitTrack,
	} from '@/utils/gitTrack';
	import { Plus, X } from 'lucide-vue-next';
	import { defineComponent, ref } from 'vue';

	const MODES = [
		{
			value: 'ask',
			labelKey: 'desktop.gitTrack.modes.ask.label',
			hintKey: 'desktop.gitTrack.modes.ask.hint',
		},
		{
			value: 'auto',
			labelKey: 'desktop.gitTrack.modes.auto.label',
			hintKey: 'desktop.gitTrack.modes.auto.hint',
		},
		{
			value: 'off',
			labelKey: 'desktop.gitTrack.modes.off.label',
			hintKey: 'desktop.gitTrack.modes.off.hint',
		},
	];

	export default defineComponent({
		name: 'DesktopGitTrackSettings',
		components: { Button, Plus, X },
		setup() {
			const mode = ref(loadGitTrackMode());
			const folders = ref(loadGitTrackFolders());
			const adding = ref(false);
			const error = ref('');

			const persist = async () => {
				try {
					await syncGitTrack();
					error.value = '';
				} catch (failure) {
					error.value = String(failure);
				}
			};

			const setMode = (value) => {
				mode.value = value;
				saveGitTrackMode(value);
				persist();
			};

			const addFolder = async () => {
				adding.value = true;
				error.value = '';
				try {
					const { invoke } = await import('@tauri-apps/api/core');
					const path = await invoke('git_track_pick_folder');
					if (!path || folders.value.includes(path)) return;
					folders.value = [...folders.value, path].slice(
						0,
						MAX_GIT_TRACK_FOLDERS,
					);
					saveGitTrackFolders(folders.value);
					await persist();
				} catch (failure) {
					error.value = String(failure);
				} finally {
					adding.value = false;
				}
			};

			const removeFolder = (folder) => {
				folders.value = folders.value.filter((item) => item !== folder);
				saveGitTrackFolders(folders.value);
				persist();
			};

			return {
				mode,
				folders,
				adding,
				error,
				modes: MODES,
				maxFolders: MAX_GIT_TRACK_FOLDERS,
				setMode,
				addFolder,
				removeFolder,
			};
		},
	});
</script>
