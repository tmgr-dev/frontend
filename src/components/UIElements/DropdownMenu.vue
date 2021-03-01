<template>
  <div class="tc-block z-10">
    <button @click.prevent="isOpenDropdownMenu = !isOpenDropdownMenu">
      <svg
        class="mr-3 md:mr-1 h-12 w-6 fill-current text-grey-dark"
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 20 20"
      >
        <path
          d="M4 12a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4zm6 0a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
        />
      </svg>
    </button>
    <button
      v-if="isOpenDropdownMenu"
      tabindex="-1"
      class="fixed z-10 inset-0 h-full w-full bg-black opacity-50 cursor-default"
      @click="isOpenDropdownMenu = false"
    />
    <div
      v-if="isOpenDropdownMenu"
      class="absolute right-0 mt-2 py-2 w-48 bg-white rounded-lg shadow-xl" style="z-index: 99"
    >
      <a
        v-for="(action, i) in actions"
        :key="i"
        href="#"
        class="tc-block px-4 py-2 text-gray-800 hover:bg-indigo-500 hover:text-white"
        @click.prevent="action.click($event)"
      >{{ action.label }}</a>
    </div>
  </div>
</template>

<script>
	export default {
		name: 'DropdownMenu',
		props: {
			actions: {
				required: true,
				type: Array
			}
		},
		data() {
			return {
				isOpenDropdownMenu: false
			}
		}
	}
</script>
