trunk build --release --features "local-dev"
wasm-opt -Oz -o dist/ff_egui_app_bg.wasm dist/ff_egui_app_bg.wasm
wasm-tools strip dist/ff_egui_app_bg.wasm -o dist/ff_egui_app_bg.wasm
