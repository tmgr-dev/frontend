<template>
    <div>
        <div v-if="counter" :class="'counter ' + extraCounterClass" id="counter">
            <span class="countdown-wrapper" title="Время: ЧЧ:ММ:СС">
                <span class="countdown-item">{{ countdown.hours }}</span>
                <span class="countdown-item">{{ countdown.minutes }}</span>
                <span :class="`countdown-item ` + (countdownInterval ? `seconds` : ``)">{{ countdown.seconds }}</span>
            </span>
                <br>
                <br>
            <Button :color="counter.start_time ? 'red' : 'blue'"
                    type="button"
                    @click="toggleCountdown"
            >{{ counter.start_time ? 'Stop' : 'Run' }}</Button>
            <Button color="white"
                    type="button"
                    class="fullscreen-toggler"
                    @click="fullscreen"
            >Fullscreen</Button>
        </div>
    </div>
</template>

<script>
    export default {
        name: "Countdown",
        props: {
            initCounter: {
                required: true,
                type: Object
            }
        },
        mounted () {
            this.counter = this.initCounter
            this.renderTime()
            this.initCountdown()
        },
        data () {
            return {
                extraCounterClass: '',
                countdownInterval: null,
                counter: {},
                countdown: {
                    hours: '00',
                    minutes: '00',
                    seconds: '00'
                }
            }
        },
        methods: {
            fullscreen () {
                if (this.extraCounterClass === '') {
                    this.extraCounterClass = 'fullscreen'
                } else {
                    this.extraCounterClass = ''
                }
            },
            toggleCountdown () {
                // $.post(route('counter.admin.' + (this.counter.start_time ? 'stop' : 'start') + '.post', {
                //     id: this.counter.id
                // })).done((counter) => {
                //     this.counter = Object.assign(this.counter, counter)
                //     this.initCountdown()
                // })
                if (!this.counter.start_time) {
                    this.counter.start_time = new Date().getTime() / 1000
                } else {
                    this.counter.start_time = 0
                }
                this.initCountdown()
            },
            plusSecond () {
                if (!this.counter) {
                    this.counter = {
                        common_time: 0
                    }
                }
                ++this.counter.common_time
                this.renderTime()
            },
            initCountdown () {
                if (this.counter.start_time === 0) {
                    clearInterval(this.countdownInterval)
                    this.countdownInterval = null
                    return
                }
                this.countdownInterval = setInterval(() => {
                    this.plusSecond()
                }, 1000)
            },
            renderTime () {
                let seconds = this.counter.common_time
                var second = seconds % 60
                var minute = (seconds - second) / 60 | 0
                var hour = minute / 60 | 0
                minute = minute - (hour * 60)

                this.countdown.hours = this.prepareClockNumber(hour)
                this.countdown.minutes = this.prepareClockNumber(minute)
                this.countdown.seconds = this.prepareClockNumber(second)
            },
            prepareClockNumber (num) {
                return num < 10 ? '0' + num : num
            }
        }
    }
</script>

<style lang="scss">
    .counter {
        align-content: center;
        text-align: center;
        padding: 23px;
        // background: #333;
        border-radius: 10px;
        margin-bottom: 50px;
        margin-top: 50px;

        .countdown-item {
            font-size: 2em;
            border-radius: 5px;
            padding: 0px 10px;
            background-color: rgb(51, 51, 51);
            box-shadow: inset 0px 0px 26px #000;
            margin-right: 15px;
            color: white;

            &.seconds {
                color: #00c300;
            }
        }

        .counter-wrappper {
            font-size: 2em;
            display: block;
        }

        button {
            display: table-cell
        }
        .do-fullscreen {
            position: absolute;
            bottom: 10px;
            right: 10px;
            cursor: pointer;
        }
    }
    .counter.fullscreen {
        position: fixed;
        width: 100vw;
        height: 100vh;
        z-index: 99999;
        top: 0;
        left: 0;
        margin: 0px;
        padding-top: 250px;
        background: #333;
        border-radius: 0px;

        .fullscreen-toggler {
            color: white;
        }
    }
</style>
