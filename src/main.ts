import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "./styles/root.css";

const app = createApp(App);
const pinia = createPinia();

app.use(pinia);

createApp(App).mount("#app");
