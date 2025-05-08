import wasm_bindgen, {load_worker_entry_point} from "./ff_egui_app.js";

self.onmessage = async event => {
    await wasm_bindgen({
        path: "./ff_egui_app_bg.wasm",
    });
    load_worker_entry_point(event.data)
}