# hello-plugin

Sample guest plugin for Alloy's WASM plugin host, compiled to `wasm32-wasip2`.

## Building

```sh
# Ensure the wasm32-wasip2 target is installed
rustup target add wasm32-wasip2

# Build the guest component
cargo build --target wasm32-wasip2 --release
```

The output will be located at `target/wasm32-wasip2/release/hello_plugin.wasm`.
This is a standard WebAssembly component implementing the `alloy:plugin` WIT world.
