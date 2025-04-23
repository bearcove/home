import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { visualizer } from "rollup-plugin-visualizer";
import path from "path";

export default defineConfig({
    base: `%BASE%/dist`,
    clearScreen: false,
    resolve: {
        alias: {
            "@home-base-fonts": path.resolve(
                __dirname,
                // we're in `.home/vite.config.json`, so `..` gets rid of the `.home`
                "../node_modules/@bearcove/home-base/src/lib/sass/fonts/",
            ),
        },
    },
    plugins: [
        wasm(),
        topLevelAwait(),
        svelte({
            inspector: {
                toggleKeyCombo: "alt-x",
                showToggleButton: "always",
                toggleButtonPos: "bottom-right",
            },
        }),
        visualizer(),
    ],
    build: {
        target: "esnext",
        outDir: "dist",
        rollupOptions: {
            input: ["src/bundle.ts"],
        },
    },
    server: {
        // just testing
        hmr: {
            host: "%SERVER_HMR_HOST%",
            clientPort: parseInt("%SERVER_HMR_CLIENT_PORT%", 10),
        },
        // https://vite.dev/config/server-options.html#server-host
        // force IPv4 here, sometimes it only listens on IPv6
        host: "127.0.0.1",
        origin: "%SERVER_ORIGIN%",
        // https://vite.dev/config/server-options.html#server-cors
        cors: {
            origin: "%SERVER_CORS_ORIGIN%",
        },
        // https://vite.dev/config/server-options.html#server-warmup
        // warmup the bundles in advance
        warmup: {
            clientFiles: ["src/bundle.ts"],
        },
        // allow serving node modules and our own sources
        fs: {},
    },
});
