// Note: this file _needs_ to be named `bundle.ts`, that's what home expects.

// This will be tree-shaken in prod, no worries. It pulls in
// the deploy UI and nice local editing experience.
import { renderAdmin } from "@bearcove/home-base";

// These are our own custom styles.
import "./main.scss";

// This whole thing gets eliminated in production — you
// can check by inspecting the bundle!
// => https://www.npmjs.com/package/vite-bundle-visualizer
if (import.meta.env.DEV) {
    renderAdmin();
}
