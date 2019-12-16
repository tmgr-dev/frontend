<template>
    <div class="overlay" :class="{ active: show }" @click="close">
        <div class="modal">
            <slot name="modal-body"></slot>
        </div>
    </div>
</template>

<script>
    export default {
        name: "Modal",
        props: {
            show: {
                type: Boolean,
                required: false,
                default: () => false
            }
        },
        methods: {
            close(e) {
                if (e.target.classList.contains("overlay")) {
                    this.$emit("close");
                }
            }
        }
    };
</script>

<style lang="scss">
    .overlay {
        position: fixed;
        top: 0;
        right: 0;
        width: 100%;
        height: 100%;
        background: rgba(0, 0, 0, 0.555);
        z-index: -1;
        transition: 0.4s;
        opacity: 0;
        transform: translate3d(100%, 0, 0);
        &.active {
            transform: translate3d(0, 0, 0);
            z-index: 100;
            opacity: 1;
            .modal {
                animation-name: bounceInRight;
                animation-duration: 1s;
                animation-fill-mode: both;
                opacity: 1;
            }
        }
    }
    .modal {
        background: #fff;
        max-width: 80%;
        margin: 50px auto;
        padding: 30px;
        nav {
            display: none;
        }

        transform: translate3d(3000px, 0, 0);
        opacity: 0;    
    }

    @keyframes bounceInRight {
        from,
        60%,
        75%,
        90%,
        to {
            animation-timing-function: cubic-bezier(0.215, 0.61, 0.355, 1);
        }

        from {
            opacity: 0;
            transform: translate3d(3000px, 0, 0);
        }

        60% {
            opacity: 1;
            transform: translate3d(-25px, 0, 0);
        }

        75% {
            transform: translate3d(10px, 0, 0);
        }

        90% {
            transform: translate3d(-5px, 0, 0);
        }

        to {
            transform: translate3d(0, 0, 0);
        }
    }
</style>
