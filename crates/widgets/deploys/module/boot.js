// Starts the module's WebAssembly.
//
// Trunk writes this loader into the page as an inline script, and the module
// CSP refuses inline script (specification HLIN-S-0007, *The module CSP*): a
// frame may run only files served from its own platform's assets. So
// `Trunk.toml` replaces Trunk's inline loader with a tag naming this file, and
// passes the hashed names of the JavaScript glue and the `.wasm` it built as
// data attributes, which this reads.
//
// A module script has no `document.currentScript`, hence the selector.
const tag = document.querySelector("script[data-wasm]");
const { default: init } = await import(tag.dataset.js);
await init({ module_or_path: tag.dataset.wasm });
