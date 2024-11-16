import wasm_bindgen, { load_worker_entry_point } from "./ffweb.js";

self.onmessage = async event => {
    await wasm_bindgen({
       path: "./ffweb_bg.wasm",
    });

    load_worker_entry_point(event.data)
}