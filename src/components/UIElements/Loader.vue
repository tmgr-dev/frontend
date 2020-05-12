<template>
    <div class="loader_wrapper" :class="[{active: isActive}, type === 'mini' ? 'mini' : '']" :style="sidePositionStyle">
        <div class="loader">
            <div></div>
            <div></div>
            <div></div>
            <div></div>
        </div>
    </div>
</template>

<script>
    export default {
        name: "Loader",
        props: {
            sidePositionStyle: {
                type: String,
                required: false,
                default: 'left: 100%'
            },
            isActive: {
                type: Boolean,
                required: true,
                default: true
            },
            type: {
                type: String,
                required: false
            }
        },
    };
</script>

<style lang="scss" scoped>
    .loader_wrapper {
        &:not(.mini) {
            display: flex;
        }
        visibility: hidden;
        opacity: 0;
        transition: opacity .2s;
        &.active {
            opacity: 1;
            visibility: visible;
        }
        &.mini {
            position: absolute;
            left: auto;
            right: auto;
            margin-left: 10px;
            margin-right: 10px;
            top: calc(50% - 7px);
            .loader {
                width: 20px;
                height: 20px;
            }

            .loader div {
                width: 12px;
                height: 12px;
                border: 2px solid;
                border-color: #fff transparent transparent transparent;
            }
        }
    }
    .loader {
        &:not(.mini) {
            margin: auto
        }
        position: relative;
        width: 60px;
        height: 60px;
        div {
            box-sizing: border-box;
            display: block;
            position: absolute;            
            width: 42px;
            height: 42px;
            border: 5px solid;
            border-radius: 50%;
            animation: lds-ring 1.2s cubic-bezier(0.5, 0, 0.5, 1) infinite;
            border-color: #808080 transparent transparent transparent;
        }

        div:nth-child(1) {
            animation-delay: -0.45s;
        }

        div:nth-child(2) {
            animation-delay: -0.3s;
        }

        div:nth-child(3) {
            animation-delay: -0.15s;
        }
    }

    @keyframes lds-ring {
        0% {
            transform: rotate(0deg);
        }
        100% {
            transform: rotate(360deg);
        }
    }
</style>
