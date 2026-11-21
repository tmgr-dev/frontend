<template>
	<div class="manager app-shell">
		<aside class="side">
			<div class="top">
				<b>{{ $t('manager.list.label') }}</b>
			</div>
			<button type="button" class="sidesrch" @click="store.openSearch()">
				<span>{{ $t('manager.search.scopeAll') }}</span>
				<kbd>&#8984;K</kbd>
			</button>
			<ChatList
				:rooms="store.rooms"
				:active-key="store.activeKey"
				:unread="store.unreadByRoom"
				@select="store.select"
			/>
		</aside>
		<section class="main">
			<ChatHeader
				v-if="activeInfo"
				:info="activeInfo"
				@search="store.openSearch(store.activeKey ?? undefined)"
			/>
			<SearchPanel
				v-if="store.search.open"
				:query="store.search.text"
				:scope="store.search.scopeKey ? 'chat' : 'all'"
				:chat-label="activeInfo?.title ?? ''"
				:hits="store.search.hits"
				:room-titles="roomTitles"
				@update:query="store.runSearch"
				@update:scope="onScope"
				@select="onHit"
			/>
			<template v-else>
				<MessageList
					ref="list"
					:messages="messages"
					:body-html="bodyHtml"
					@reply="store.setReply"
					@copy="copy"
				>
					<template #decision="{ message }">
						<div class="decision-ref">
							{{ message.decision?.card_id }}
						</div>
					</template>
				</MessageList>
				<Composer
					v-model="store.draft.text"
					:reply-to="store.replyTo ?? undefined"
					:attachments="store.draft.attachments"
					@update:model-value="store.onInput"
					@send="store.send"
					@clear-reply="store.setReply(null)"
					@remove-attachment="store.removeAttachment"
					@attach="pick"
					@drop="addFiles"
					@keydown="onKey"
				>
					<template #popover
						><SlashMenu
							v-if="store.slash.open"
							:commands="store.slash.items"
							:active-index="store.slash.index"
							@select="store.pickSlash"
					/></template>
				</Composer>
			</template>
			<p v-if="store.error" class="mgr-error" role="alert">
				{{ store.error }}
			</p>
		</section>
		<input ref="file" type="file" multiple hidden @change="onFiles" />
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import ChatHeader from './components/ChatHeader.vue';
	import ChatList from './components/ChatList.vue';
	import Composer from './components/Composer.vue';
	import MessageList from './components/MessageList.vue';
	import SearchPanel from './components/SearchPanel.vue';
	import SlashMenu from './components/SlashMenu.vue';
	import { renderBody } from './format';
	import { useManagerStore } from './store';
	import { roomKey } from './types';

	const sha256Hex = async (file: File): Promise<string> => {
		const digest = await crypto.subtle.digest(
			'SHA-256',
			await file.arrayBuffer(),
		);
		return Array.from(new Uint8Array(digest), (b) =>
			b.toString(16).padStart(2, '0'),
		).join('');
	};

	export default defineComponent({
		name: 'ManagerApp',
		components: {
			ChatHeader,
			ChatList,
			Composer,
			MessageList,
			SearchPanel,
			SlashMenu,
		},
		setup() {
			return { store: useManagerStore() };
		},
		computed: {
			activeInfo() {
				return this.store.rooms.find(
					(r) => roomKey(r.room) === this.store.activeKey,
				);
			},
			messages() {
				return this.store.activeKey
					? (this.store.messagesByRoom[this.store.activeKey] ?? [])
					: [];
			},
			bodyHtml(): Record<string, string> {
				return Object.fromEntries(
					this.messages.map((m) => [m.id, renderBody(m.body_md)]),
				);
			},
			roomTitles(): Record<string, string> {
				return Object.fromEntries(
					this.store.rooms.map((r) => [roomKey(r.room), r.title]),
				);
			},
		},
		mounted() {
			void this.store.init();
			window.addEventListener('keydown', this.onGlobalKey);
		},
		beforeUnmount() {
			window.removeEventListener('keydown', this.onGlobalKey);
		},
		methods: {
			onGlobalKey(e: KeyboardEvent) {
				if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
					e.preventDefault();
					this.store.openSearch();
				} else if (e.key === 'Escape' && this.store.search.open) {
					this.store.closeSearch();
				}
			},
			onKey(e: KeyboardEvent) {
				if (!this.store.slash.open) return;
				if (e.key === 'ArrowDown') {
					e.preventDefault();
					this.store.moveSlash(1);
				} else if (e.key === 'ArrowUp') {
					e.preventDefault();
					this.store.moveSlash(-1);
				} else if (e.key === 'Tab' || e.key === 'Enter') {
					e.preventDefault();
					this.store.pickSlash();
				} else if (e.key === 'Escape') {
					this.store.closeSlash();
				}
			},
			onScope(scope: 'chat' | 'all') {
				this.store.search.scopeKey =
					scope === 'chat' ? this.store.activeKey : null;
				void this.store.runSearch(this.store.search.text);
			},
			async onHit(hit: Parameters<typeof this.store.jumpToHit>[0]) {
				this.store.closeSearch();
				await this.store.jumpToHit(hit);
			},
			copy(text: string) {
				void navigator.clipboard?.writeText(text);
			},
			pick() {
				(this.$refs.file as HTMLInputElement).click();
			},
			onFiles(e: Event) {
				const input = e.target as HTMLInputElement;
				void this.addFiles(Array.from(input.files ?? []));
				input.value = '';
			},
			async addFiles(files: File[]) {
				for (const file of files) {
					this.store.addAttachment({
						name: file.name,
						type: file.type || 'application/octet-stream',
						size: file.size,
						sha256: await sha256Hex(file),
					});
				}
			},
		},
	});
</script>

<style src="./manager.css"></style>
