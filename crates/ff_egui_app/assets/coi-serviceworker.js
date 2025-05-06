/*! fluxfox-coi-serviceworker v0.2 Original by Guido Zuidhof and contributors, licensed under MIT */
let coepCredentialless = false;

if (typeof window === 'undefined') {
    self.addEventListener("install", () => self.skipWaiting());
    self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

    self.addEventListener("message", (ev) => {
        if (!ev.data) return;

        switch (ev.data.type) {
            case "deregister":
                self.registration.unregister()
                    .then(() => self.clients.matchAll())
                    .then(clients => {
                        for (const client of clients) {
                            client.postMessage({type: "reload"});
                        }
                    });
                break;

            case "coepCredentialless":
                coepCredentialless = ev.data.value;
                break;
        }
    });

    self.addEventListener("fetch", (event) => {
        const r = event.request;

        // Bail out on no-cors mode; not safe to modify
        if (r.mode === "no-cors" || (r.cache === "only-if-cached" && r.mode !== "same-origin")) {
            return;
        }

        const request = coepCredentialless
            ? new Request(r, {credentials: "omit"})
            : r;

        event.respondWith(
            fetch(request).then((response) => {
                if (response.status === 0 || !response.ok) {
                    return response;
                }

                const headers = new Headers(response.headers);
                headers.set("Cross-Origin-Embedder-Policy", coepCredentialless ? "credentialless" : "require-corp");
                headers.set("Cross-Origin-Opener-Policy", "same-origin");

                if (!coepCredentialless) {
                    headers.set("Cross-Origin-Resource-Policy", "cross-origin");
                }

                const responseClone = response.clone();
                return responseClone.blob().then((body) =>
                    new Response(body, {
                        status: response.status,
                        statusText: response.statusText,
                        headers,
                    })
                );
            }).catch(console.error)
        );
    });
} else {
    (() => {
        const reloadedBySelf = sessionStorage.getItem("coiReloadedBySelf");
        sessionStorage.removeItem("coiReloadedBySelf");

        const coepDegrading = (reloadedBySelf === "coepdegrade");

        const coi = {
            shouldRegister: () => !reloadedBySelf,
            shouldDeregister: () => false,
            coepCredentialless: () => true,
            coepDegrade: () => true,
            doReload: () => location.reload(),
            quiet: false,
            ...window.coi,
        };

        const n = navigator;

        if (window.crossOriginIsolated || !coi.shouldRegister()) return;
        if (!window.isSecureContext) {
            !coi.quiet && console.warn("Secure context required for COOP/COEP.");
            return;
        }
        if (!n.serviceWorker) {
            !coi.quiet && console.warn("No service worker support (maybe private mode?).");
            return;
        }

        const coepHasFailed = sessionStorage.getItem("coiCoepHasFailed");
        const reloadToDegrade = coi.coepDegrade() && !coepDegrading && !window.crossOriginIsolated;

        n.serviceWorker.register(document.currentScript.src).then((registration) => {
            !coi.quiet && console.log("COOP/COEP SW registered", registration.scope);

            registration.addEventListener("updatefound", () => {
                !coi.quiet && console.log("Reloading to activate updated COOP/COEP SW.");
                sessionStorage.setItem("coiReloadedBySelf", "updatefound");
                coi.doReload();
            });

            if (!n.serviceWorker.controller) {
                n.serviceWorker.addEventListener("controllerchange", () => {
                    !coi.quiet && console.log("Controller active – reloading for COOP/COEP.");
                    sessionStorage.setItem("coiReloadedBySelf", "controllerchange");
                    coi.doReload();
                });
            } else {
                if (!window.crossOriginIsolated) {
                    sessionStorage.setItem("coiCoepHasFailed", "true");
                } else {
                    sessionStorage.removeItem("coiCoepHasFailed");
                }

                n.serviceWorker.controller.postMessage({
                    type: "coepCredentialless",
                    value: (reloadToDegrade || (coepHasFailed && coi.coepDegrade()))
                        ? false
                        : coi.coepCredentialless(),
                });

                if (coi.shouldDeregister()) {
                    n.serviceWorker.controller.postMessage({type: "deregister"});
                }
            }
        }).catch((err) => {
            !coi.quiet && console.error("COOP/COEP SW registration failed:", err);
        });

        // Listen for reload trigger from service worker
        n.serviceWorker.addEventListener("message", (event) => {
            if (event.data && event.data.type === "reload") {
                location.reload();
            }
        });
    })();
}