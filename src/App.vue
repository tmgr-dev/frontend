<template>
	<div :class="$color('textMain')">
		<transition
			name="fade"
			mode="out-in"
		>
			<Navbar v-if="$route.meta.navbarHidden" />
		</transition>

		<transition
			:name="transitionName"
			mode="out-in"
			@beforeLeave="beforeLeave"
			@enter="enter"
			@afterEnter="afterEnter"
		>
			<router-view
				:key="$route.path"
				v-slot="{ Component }"
			>
				<component :is="Component" />
			</router-view>
		</transition>
	</div>
</template>

<script>
	import Navbar from "@/components/UIElements/Navbar";

	const DEFAULT_TRANSITION = 'fade'

	export default {
		name: 'App',
		components: {
			Navbar
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
			};
		},
		computed: {
			navbarHidden () {
				return this.$route.name !== 'Index'
			}
		},
		created() {
			this.$router.beforeEach((to, from, next) => {
				let transitionName = to.meta.transitionName || from.meta.transitionName;

				if (transitionName === 'slide') {
					const toDepth = to.path.split('/').length;
					const fromDepth = from.path.split('/').length;
					transitionName = toDepth < fromDepth ? 'slide-right' : 'slide-left';
				}

				this.transitionName = transitionName || DEFAULT_TRANSITION;

				next();
			});
		},
		methods: {
			beforeLeave(element) {
				this.prevHeight = getComputedStyle(element).height;
			},
			enter(element) {
				const {height} = getComputedStyle(element);

				element.style.height = this.prevHeight;

				setTimeout(() => {
					element.style.height = height;
				});
			},
			afterEnter(element) {
				element.style.height = 'auto';
			},
		}
	}
</script>

<style src="./assets/styles/index.scss" lang="scss"></style>
