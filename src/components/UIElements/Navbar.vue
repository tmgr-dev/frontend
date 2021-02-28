<template>
  <div class="ml-auto md:hidden absolute z-30 right-0 p-8 pr-4">
    <a href="#" class="toggle-button" @click.prevent="$store.getters.slideout.toggle()">
      <svg fill="currentColor" viewBox="0 0 20 20" class="w-6 h-6">
        <!--  @click="isHidden = !isHidden" -->
        <path v-show="isHidden" fill-rule="evenodd" d="M3 5a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM3 10a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM9 15a1 1 0 011-1h6a1 1 0 110 2h-6a1 1 0 01-1-1z" clip-rule="evenodd"></path>
        <path v-show="!isHidden" fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"></path>
      </svg>
    </a>
  </div>
	<nav :class="`${$color('menuBg')} shadow md:block tc-hidden`" role="navigation">
		<div class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap">
			<div :class="`w-full md:w-auto md:flex-grow md:flex md:items-center ${isHidden ? 'tc-hidden' : ''}`">
				<navbar-menu />
				<span :class="`${$color('navTextUser')}-500`" class="flex justify-between items-center ml-auto mr-4" :key="rerenderSwitcher">
					<day-night-switch v-model="switchOn"/>
				</span>
				<account-dropdown />
			</div>
		</div>
	</nav>
</template>

<script>

	import DayNightSwitch from './DayNightSwitch'
	import AccountDropdown from './AccountDropdown'
	import NavbarMenu from "src/components/UIElements/NavbarMenu";

	export default {
		name: 'Navbar',
		components: {
			NavbarMenu,
			AccountDropdown,
			DayNightSwitch
		},
    created() {
			console.log('Navbar created')
      window.addEventListener("test", null, Object.defineProperty({}, 'passive', {
        get: () => this.supportsPassive = true
      }));
    },
    watch: {
		  isHidden(newVal) {
		    if (newVal) {
          return this.enableScroll()
        }
		    return this.disableScroll()
      }
    },
		data: () => ({
			isHidden: true,
			rerenderSwitcher: 0,
			links: [
				{ id: 1, name: 'Current tasks', path: '/' },
				{ id: 1, name: 'tc-hidden', path: '/tc-hidden' },
				{ id: 1, name: 'Archive', path: '/acrhive' },
				{ id: 1, name: 'Categories', path: '/projects-categories' },
			],
      wheelEvent: 'onwheel' in document.createElement('div') ? 'wheel' : 'mousewheel',
      supportsPassive: false
    }),
		computed: {
      wheelOpt () {
        return this.supportsPassive ? { passive: false } : false
      },
			switchOn: {
				get () {
					return this.$store.getters.colorScheme === 'dark'
				},
				set (newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default')
				}
			}
		},
    methods: {
      preventDefault(e) {
        e.preventDefault();
      },
      preventDefaultForScrollKeys(e) {
        if (keys[e.keyCode]) {
          this.preventDefault(e);
          return false;
        }
      },
      disableScroll() {
        window.addEventListener('DOMMouseScroll', this.preventDefault, false); // older FF
        window.addEventListener(this.wheelEvent, this.preventDefault, this.wheelOpt); // modern desktop
        window.addEventListener('touchmove', this.preventDefault, this.wheelOpt); // mobile
        window.addEventListener('keydown', this.preventDefaultForScrollKeys, false);
      },
      enableScroll() {
        window.removeEventListener('DOMMouseScroll', this.preventDefault, false);
        window.removeEventListener(this.wheelEvent, this.preventDefault, this.wheelOpt);
        window.removeEventListener('touchmove', this.preventDefault, this.wheelOpt);
        window.removeEventListener('keydown', this.preventDefaultForScrollKeys, false);
      }
    }
	}
</script>

<style>
.fade-kiosk-list-enter-active,
.fade-kiosk-list-leave-active {
  transition: all 0.9s ease-in-out;
}

.mobile-navbar-menu {
  /*transform: translateX(-50%);*/
}

.fade-kiosk-list-enter {
  transform: translateX(100%);
}

.fade-kiosk-list-leave-to {
  transform: translateX(-100%);
}
</style>
