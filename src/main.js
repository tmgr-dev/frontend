import Vue from './bootstrap/index'
import VueRouter from 'vue-router'
import routes from './routes/index'
import App from './App'
import './assets/styles/index.css';

Vue.use(VueRouter)
Vue.config.productionTip = false
const router = new VueRouter({
    routes,
    mode: 'history'
})

new Vue({
  router,
  render: h => h(App),
    components: {

    },
}).$mount('#app')
