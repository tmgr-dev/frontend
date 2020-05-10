<template>
  <div id="app" :class="$color('textMain')">
      <router-view :key="$route.path"></router-view>
  </div>
</template>

<script>

export default {
  name: 'App'
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
.bg-body {
    background-color: #edf2f7;
}
.dark-bg-body {
    background-color: #121212;
}
</style>
