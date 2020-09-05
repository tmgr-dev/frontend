<template>
    <router-link
            v-if="type === 'router-link'"
            :to="to"
            class="mt-3 sm:mt-0 py-2 px-5 md:py-3 md:px-6 font-bold text-white rounded-lg shadow-md"
            :class="`bg-${color}-700 hover:bg-${color}-600`"
    >
        <slot></slot>
    </router-link>
    <button
            class="mt-3 sm:mt-0 py-2 px-5 md:py-3 md:px-6 font-bold text-white rounded-lg shadow-md"
            :class="`bg-${color}-700 hover:bg-${color}-600`"
            v-else
            :type="type"
            @click="$emit('click')"
    >
        <slot></slot>
    </button>
</template>

<script>
    export default {
        name: "NewButton",
        props: {
            type: {
                required: false,
                type: String,
                default: () => 'button'
            },
            to: {
                required: false,
                type: String,
                default: () => ''
            },
            color: {
                required: false,
                type: String,
                default: () => 'green'
            }
        },
        methods: {
            getClass () {
                return `${this.extraClass} px-5 py-1 border border-4 border-${this.color}-400 text-${this.color}-600 hover:bg-${this.color}-400 hover:text-white`
            }
        }
    }
</script>

<style scoped>

</style>
