<template>
    <router-link v-if="type === 'router-link'" :to="to" :class="getClass()"><slot></slot></router-link>
    <button v-else :type="type" @click="$emit('click')" :class="getClass()"><slot></slot></button>
</template>

<script>
    export default {
        name: "Button",
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
            },
            extraClass: {
                required: false,
                type: String,
                default: () => ''
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
