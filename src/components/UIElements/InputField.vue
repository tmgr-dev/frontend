<template>
    <div>
        <input
            :id="name"
            :type="type"
            :class="`flex-grow h-8 w-full px-2 border rounded border-grey-400 ${errors ? 'with-errors' : ''}`"
            :name="name"
            :placeholder="placeholder"
            v-model="val"
        >
        <div v-if="errors"  class="error">
            <div v-for="message in errors" v-bind:key="message">
                {{ message }}
            </div>
        </div>
    </div>
</template>

<script>
    export default {
        name: "InputField",
        props: {
            value: {
                required: false,
                type: String
            },
            errors: {
                required: false,
                type: Array,
                default: () => null
            },
            type: {
                required: false,
                type: String,
                default: () => 'text'
            },
            placeholder: {
                required: false,
                type: String,
                default: () => ''
            },
            name: {
                required: true,
                type: String
            }
        },
        computed: {
            val: {
                get () { return this['value'] },
                set (v) {
                    this.$emit('update:value', v)
                }
            }
        },
        methods: {
        }
    }
</script>

<style scoped lang="scss">
    input.with-errors {
        border: red solid 1px;
    }
    .error {
        font-size: .9em;
        color: red;
    }
</style>
