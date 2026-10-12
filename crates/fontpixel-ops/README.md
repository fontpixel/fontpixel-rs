# fontpixel-ops

The operations of the [`fontpixel`](https://crates.io/crates/fontpixel-cli)
command-line tool (`info`, `convert`, `merge`, `variants`, `subset`,
`preview`, `render`, `batch`, ...) as a typed Rust API. The CLI and the
Python and JavaScript bindings share this crate, so a request gives the same
bytes, reports and exit codes everywhere.

- Typed requests and results, serializable with serde
- Two storage modes: in-memory files (`{name: bytes}`), or a sandboxed
  directory (feature `fs`, through `cap-std`), with all-or-nothing output
  groups and protected sources
- Batch jobs from TOML (feature `toml`), parsed with the same definition as the
  command line (feature `clap`)
- Progress callbacks and cooperative cancellation

```toml
[dependencies]
fontpixel-ops = "0.1"
```

Format features mirror those of the [`fontpixel`](https://crates.io/crates/fontpixel)
crate (`bdf`, `pcf`, `opentype`, `all-formats`, ...); the defaults enable all
formats plus `fs`. For `wasm32-unknown-unknown`, disable the default features
(`fs` needs an operating system) and select formats explicitly.

API reference: <https://docs.rs/fontpixel-ops>. Repository:
<https://github.com/fontpixel/fontpixel-rs>.

## License

MIT
