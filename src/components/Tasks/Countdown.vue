<template>
    <div>
        <div v-if="task" :class="'task ' + extraCounterClass" id="task">
            <span class="countdown-wrapper" title="Время: ЧЧ:ММ:СС">
                <span class="countdown-item">{{ countdown.hours }}</span>
                <span class="countdown-item">{{ countdown.minutes }}</span>
                <span :class="`countdown-item ` + (countdownInterval ? `seconds` : ``)">{{ countdown.seconds }}</span>
            </span>
                <br>
                <br>
            <Button :color="task.start_time ? 'red' : 'blue'"
                    type="button"
                    @click="toggleCountdown"
            >{{ task.start_time ? 'Stop' : 'Run' }}</Button>
            <Button color="white"
                    type="button"
                    class="fullscreen-toggler hover:bg-gray-600"
                    @click="fullscreen"
            >Fullscreen</Button>
        </div>
    </div>
</template>

<script>
    export default {
        name: "Countdown",
        props: {
            initTask: {
                required: true,
                type: Object
            }
        },
        mounted () {
            this.task = this.initTask
            this.task.start_time = this.task.start_time || 0

            this.initCountdown()
						this.renderTime()

            this.$on('update-task', task => {
                this.task = task
                this.initCountdown()
            })
        },
        data () {
            return {
                extraCounterClass: '',
                countdownInterval: null,
                countdown: {
                    hours: '00',
                    minutes: '00',
                    seconds: '00'
                },
                task: null
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
                this.$emit('toggle')
            },
            plusSecond () {
                if (!this.task) {
                    this.task = {
                        common_time: 0
                    }
                }
                ++this.task.common_time
                this.$emit('update:seconds', this.task.common_time)
                this.renderTime()
            },
            prepareCommonTime () {
                if (!this.task.common_time) {
                    return
                }
                const currentTime = new Date().getTime() / 1000
                this.task.common_time += currentTime.toFixed(1) - this.task.start_time
                this.task.common_time = Math.floor(this.task.common_time)
            },
            initCountdown () {
                if (!this.task.start_time) {
                    clearInterval(this.countdownInterval)
                    this.countdownInterval = null
                    return
                }

                this.prepareCommonTime()
                this.countdownInterval = setInterval(() => {
                    this.plusSecond()
                }, 1000)
            },
            renderTime () {
                let seconds = this.task.common_time
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
    .task {
        align-content: center;
        text-align: center;
        padding: 0px 23px;
        // background: #333;
        border-radius: 10px;
        @media screen and (min-width: 821px) {
					margin-bottom: 50px;
					margin-top: 50px;
					padding: 23px;
				}

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

        .task-wrappper {
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
    .task.fullscreen {
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
