<template>
    <div class="input_wrapper">
        <input
            :id="name"
            :type="type"
            :class="`flex-grow h-8 w-full px-2 border rounded border-grey-400 ${errors ? 'with-errors' : ''}`"
            :name="name"
            :placeholder="placeholder"
            v-model="val"
        >
        <transition name="fade-left">
            <div v-if="errors"  class="error" :class="{ 'tooltip': errorAsTooltip }">
                {{ errors[0] }}
            </div>
        </transition>
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
                default: null
            },
            type: {
                required: false,
                type: String,
                default: 'text'
            },
            placeholder: {
                required: false,
                type: String,
                default: ''
            },
            name: {
                required: true,
                type: String
            }
        },
        data() {
            return {
                screenWidth: null
            }
        },
        computed: {
            val: {
                get () { return this['value'] },
                set (v) {
                    this.$emit('update:value', v)
                }
            },
            errorAsTooltip() {
                return this.screenWidth <= 767 ? false : true
            }
        },
        methods: {
            updateWidth() {
                this.screenWidth = window.innerWidth;
            },
        },
        created() {
            this.updateWidth()
            window.addEventListener('resize', this.updateWidth);
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
        &.tooltip {
            position: absolute;
            left: 108%;
            white-space: nowrap;
            background: #fff;
            padding: 0.75rem;
            min-width: 220px;
            display: inline-block;
            top: -6px;
            border-radius: 7px;
            box-shadow: 1px 1px 4px #888;
            line-height: 1.1;
            div {
                margin-bottom: 5px;
            }
            &:before {
                content: '';
                border: 6px solid transparent;
                border-right-color: #ffffff;
                position: absolute;
                left: -12px;
                z-index: 2;
                top: calc(50% - 5px);
            }
            &:after {
                content: '';
                border: 8px solid transparent;
                border-right-color: #d0d0d0;
                position: absolute;
                top: calc(50% - 7px);
                left: -16px;    
            }
        }
    }
    .input_wrapper {
        position: relative;
    }

    .fade-left-enter-active, .fade-left-leave-active {
        transition: .5s;
        transform: translateX(0);
    }
    .fade-left-enter, .fade-left-leave-to {
        transform: translateX(25px);
    }
</style>
