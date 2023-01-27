<template>
	<alert ref="alert" />
	<div id="q-app" class="text-tmgr-blue dark:text-tmgr-gray">
		<q-bar
			v-if="$q.platform.is.electron"
			class="q-electron-draggable relative w-full p-2"
			style="background-color: inherit"
		>
			<img class="inline" src="/favicon-16x16.png" />

			<div class="inline float-right">
				<q-btn
					class="q-electron-drag--exception"
					dense
					flat
					icon="minimize"
					@click="minimize"
				/>

				<q-btn
					class="q-electron-drag--exception"
					dense
					flat
					icon="crop_square"
					@click="maximize"
				/>

				<q-btn
					class="q-electron-drag--exception"
					dense
					flat
					icon="close"
					@click="closeApp"
				/>
			</div>
		</q-bar>

		<Slideout
			menu="#menu"
			panel="#panel"
			side="right"
			@on-translate="translateMenu"
			@on-beforeclose="menuIsActive = false"
			@on-beforeopen="menuIsActive = true"
		>
			<div id="menu" class="overflow-y-hidden" style="overflow-y: hidden">
				<div class="z-20 px-4 text-right">
					<navbar-menu />
					<account-dropdown class="flex justify-end" />
					<span
						class="absolute bottom-0 right-0 pr-4 pb-10 text-black dark:text-white"
					>
						<day-night-switch v-model="switchOn" />
					</span>
				</div>
			</div>

			<div id="panel">
				<q-scroll-area
					:style="{
						height: bodyHeight + 'px',
					}"
				>
					<transition mode="out-in" name="fade">
						<div>
							<Navbar
								v-if="$route.meta.navbarHidden"
								:menu-is-active="menuIsActive"
								:menu-position="translateMenuPosition"
							/>
						</div>
					</transition>

					<router-view :key="$route.path" v-slot="{ Component }">
						<transition
							:name="transitionName"
							mode="out-in"
							@before-leave="beforeLeave"
							@enter="enter"
							@after-enter="afterEnter"
						>
							<div>
								<component :is="Component" v-show="showComponent"></component>
							</div>
						</transition>
					</router-view>
				</q-scroll-area>
			</div>
		</Slideout>

		<div class="fixed left-0 bottom-0 ml-10 mb-10 mr-2 z-10">
			<span v-for="task in activeTasks" class="mb-5 inline-block">
				<transition mode="out-in" name="fade">
					<a
						v-if="task.id !== $store.getters.currentOpenedTaskId"
						:href="`/${task.id}/edit`"
						@click.prevent="$store.commit('currentTaskIdForModal', task.id)"
					>
						<span
							class="relative inline-flex rounded-md shadow-sm p-2 mr-5 bg-gray-200 transition-colors duration-300 dark:bg-gray-800"
						>
							<span class="flex absolute h-5 w-5 top-0 left-0 -mt-2 -ml-2">
								<span
									class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"
								/>
								<span
									class="relative inline-flex rounded-full h-5 w-5 bg-green-500"
								/>
							</span>

							<span class="text-tmgr-blue dark:text-tmgr-gray">
								{{ task.title }}
							</span>
						</span>
					</a>
				</transition>
			</span>
		</div>

		<Transition name="bounce-right-fade">
			<modal
				name="Task"
				v-if="
					$store.getters.currentTaskIdForModal ||
					$store.getters.showCreateTaskModal
				"
				close-on-bg-click
				modal-class="w-11/12 h-full"
				@close="$store.dispatch('closeTaskModal')"
			>
				<template #modal-body>
					<task-form
						:is-modal="true"
						:modal-project-category-id="
							$store.getters.createTaskInProjectCategoryId
						"
						:modal-task-id="$store.getters.currentTaskIdForModal"
						:status-id="$store.getters.createTaskInStatusId"
						@close="$store.dispatch('closeTaskModal')"
					/>
				</template>
			</modal>
		</Transition>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';

	import Navbar from 'src/components/UIElements/Navbar';
	import NavbarMenu from 'src/components/UIElements/NavbarMenu';
	import Slideout from 'src/components/UIElements/Slideout/Slideout';
	import TaskForm from 'src/components/Tasks/TaskForm';

	const DEFAULT_TRANSITION = 'fade';

	// TODO: solve slideout & horizontal scroll in the Board
	export default defineComponent({
		name: 'App',
		components: {
			TaskForm,
			Navbar,
			Slideout,
			NavbarMenu,
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
				showComponent: true,
				activeTasks: [],
				bodyOverflow: '',
				bodyHeight: 800,
				menuIsActive: false,
				translateMenuPosition: 0,
			};
		},
		computed: {
			navbarHidden() {
				return this.$route.name !== 'Index';
			},
			switchOn: {
				get() {
					return this.$store.getters.colorScheme === 'dark';
				},
				set(newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default');
				},
			},
		},
		watch: {
			'$route.path'() {
				this.showComponent = false;
				setTimeout(() => (this.showComponent = true), 100);
			},
		},
		methods: {
			closeTaskModal() {
				this.$store.dispatch('closeTaskModal');
			},
			translateMenu(data) {
				this.translateMenuPosition = data;
			},
			beforeLeave(element) {
				this.prevHeight = getComputedStyle(element).height;
			},
			enter(element) {
				const { height } = getComputedStyle(element);

				element.style.height = this.prevHeight;

				setTimeout(() => {
					element.style.height = height;
				});
			},
			afterEnter(element) {
				element.style.height = 'auto';
			},
			async loadActiveTasks() {
				if (!this.$store.getters.user) {
					return;
				}
				const {
					data: { data },
				} = await this.$axios.get('/tasks/runned');
				this.activeTasks = data;
			},
			minimize() {
				if (process.env.MODE === 'electron') {
					window.myWindowAPI.minimize();
				}
			},
			maximize() {
				if (process.env.MODE === 'electron') {
					window.myWindowAPI.toggleMaximize();
				}
			},
			closeApp() {
				if (process.env.MODE === 'electron') {
					window.myWindowAPI.close();
				}
			},
			initBodyHeight() {
				setTimeout(() => {
					try {
						this.bodyHeight = this.getBodyHeight();
					} catch (e) {
						setTimeout(() => (this.bodyHeight = this.getBodyHeight()), 1000);
					}
				}, 500);
			},
			getBodyHeight() {
				return (
					this.getOffsetHeightOfElement('body') +
					30 -
					this.getOffsetHeightOfElement('[role=toolbar]') -
					this.getOffsetHeightOfElement('nav')
				);
			},
			getOffsetHeightOfElement(selector) {
				const el = document.querySelector(selector);
				if (!el) {
					return 0;
				}
				return el.offsetHeight;
			},
		},
		async created() {
			this.$store.dispatch('loadUserSettings');
			this.$store.dispatch('loadStatuses');

			this.$router.beforeEach((to, from, next) => {
				this.$store.commit('currentOpenedTaskId', null);
				this.loadActiveTasks();
				let transitionName = to.meta.transitionName || from.meta.transitionName;

				if (transitionName === 'slide') {
					const toDepth = to.path.split('/').length;
					const fromDepth = from.path.split('/').length;
					transitionName = toDepth < fromDepth ? 'slide-right' : 'slide-left';
				}

				this.transitionName = transitionName || DEFAULT_TRANSITION;

				next();
			});

			if (!this.$store.getters.user) {
				return;
			}
			this.$store.getters.pusherBeamsClient.getUserId().then((userId) => {
				if (!userId) {
					return this.$store.commit('pusherBeamsUserId', userId);
				}
				userId = this.$store.getters.user.id.toString();
				this.$store.getters.pusherBeamsClient.start().then(() => {
					this.$store.getters.pusherBeamsClient
						.setUserId(userId, this.$store.getters.pusherTokenProvider)
						.then(() => {
							this.$store.commit('pusherBeamsUserId', userId);
						});
				});
			});
			this.loadActiveTasks();
			if (process.env.MODE === 'electron') {
				document.body.style.overflow = 'hidden';
			}
		},
		mounted() {
			this.initBodyHeight();
		},
	});
</script>

<style lang="scss" src="src/assets/styles/index.scss"></style>
