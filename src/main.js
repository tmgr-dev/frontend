import { createApp } from "vue";
import App from "./App";
import router from "./routes/index";
import store from "./store";
import { axios, color } from "./bootstrap/globalProperties";
import VueTheMask from "vue-the-mask";
import { mask } from "vue-the-mask";
import Tooltip from "vue-directive-tooltip";
import components from "@/bootstrap/globalComponents";

const app = createApp(App, {
	data: () => ({
		key: null
	})
})
app.use(store)
app.use(router)

components.map(c => app.component(c.name, c))

app.config.globalProperties.$axios = axios
app.config.globalProperties.$color = color

app.directive(mask)
app.use(Tooltip, {
  delay: 50,
  placement: 'top',
  class: 'custom-tooltip',
  triggers: ['hover'],
  offset: 5
})
app.use(VueTheMask)

router.isReady().then(() => {
	app.mount('#app')
})
