import Vue from './bootstrap/index'
import router from './routes/index'
import App from './App'
import './assets/styles/index.css';

Vue.config.productionTip = false

new Vue({
  router,
  render: h => h(App),
    components: {

    },
}).$mount('#app')
