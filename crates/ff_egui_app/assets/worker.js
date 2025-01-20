import wasm_bindgen, {closure_worker_entry_point} from "./ff_egui_app.js";

self.onmessage = async event => {
    console.log("In worker.js with event.data[0] = " + event.data[0] + " and event.data[1] = " + event.data[1]);
    await wasm_bindgen({
        path: "./ff_egui_app_bg.wasm",
        memory: event.data[0],
    });

    //closure_worker_entry_point(Number(event.data[1]))
    closure_worker_entry_point(Number(event.data[1]))
}