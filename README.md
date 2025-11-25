![Gameplay clip](clip.gif)

# Rust + WebAssembly Tetris (1v1 vs coldclear2)
My motivation for building this was having a platform to play against a tetris bot with wacky randomizers such as 5-bag which is everything but S and Z pieces.
100% guideline compatible, with all kicks and SRS.
## Building
1) Install the wasm target and wasm-pack if you have not yet:
```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```
2) Build the WASM bundle into the web folder:
```bash
wasm-pack build --target web --out-dir web/pkg
```
3) Serve the `web` directory with the built-in Rust static server:
```bash
cargo run --bin server
```
4) Run the cold-clear-2 bridge:
```bash
cargo run --bin bot_bridge -- --listen 127.0.0.1:9000 --bot-path cold-clear-2/target/release/cold-clear-2.exe
```
5) Open `http://localhost:8080` in a browser. Use the controls panel to change bindings, randomizers, and bot PPS. Settings persist automatically.



