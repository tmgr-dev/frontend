<template>
    <button :type="type" :class="`${extraClass} px-5 py-1 border border-4 border-${color}-400 text-${color}-600 hover:bg-${color}-400 hover:text-white`">
        <slot></slot>
    </button>
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
        }
    }
</script>

<style scoped>

</style>
