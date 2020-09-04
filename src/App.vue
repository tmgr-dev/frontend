<template>
  <div id="app" :class="$color('textMain')">
      <transition
              :name="transitionName"
              mode="out-in"
              @beforeLeave="beforeLeave"
              @enter="enter"
              @afterEnter="afterEnter"
      >
        <router-view :key="$route.path"></router-view>
      </transition>
  </div>
</template>

<script>

const DEFAULT_TRANSITION = 'fade'

export default {
  name: 'App',
    data() {
        return {
            prevHeight: 0,
            transitionName: DEFAULT_TRANSITION,
        };
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
            const { height } = getComputedStyle(element);

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

<style>
#app {
  font-family: 'Roboto', sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  text-align: center;
}
.text-main {
    color: #2c3e50;
}
.dark-text-main {
    color: #e2e2e2;
}
.dark-bg-body {
    background-color: #121212 !important;
}
.bg-body {
    background-color: #edf2f7;
}

.fade-enter-active,
.fade-leave-active {
    transition-duration: 0.3s;
    transition-property: height, opacity;
    transition-timing-function: ease;
    overflow: hidden;
}

.fade-enter,
.fade-leave-active {
    opacity: 0
}

.slide-left-enter-active,
.slide-left-leave-active,
.slide-right-enter-active,
.slide-right-leave-active {
    transition-duration: 0.5s;
    transition-property: height, opacity, transform;
    transition-timing-function: cubic-bezier(0.55, 0, 0.1, 1);
    overflow: hidden;
}

.slide-left-enter,
.slide-right-leave-active {
    opacity: 0;
    transform: translate(2em, 0);
}

.slide-left-leave-active,
.slide-right-enter {
    opacity: 0;
    transform: translate(-2em, 0);
}
</style>
