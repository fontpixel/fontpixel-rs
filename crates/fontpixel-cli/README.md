# fontpixel-cli

`fontpixel`, a command-line tool for bitmap fonts: inspect, convert, merge,
subset, preview and render fonts in 99 formats (BDF, PCF, PSF, YAFF, HEX,
FNT/FON, BMFont, image atlases, C headers, OpenType bitmap and outline fonts,
WOFF/WOFF2, and many historical formats).

```sh
cargo install fontpixel-cli          # installs the `fontpixel` binary
```

```sh
fontpixel formats                                    # every format and its capabilities
fontpixel info font.bdf --json
fontpixel convert font.bdf -o font.pcf.gz
fontpixel convert font.bdf -o font.ttf --strict     # refuse any information loss
fontpixel variants --base SC.bdf --lang ja=JP.bdf -o regional.woff2
fontpixel subset regional.woff2 --text 'Hello' -o small.woff2
fontpixel merge latin.bdf cjk.bdf -o merged.bdf
fontpixel preview font.bdf --text 'Hello'
fontpixel render font.bdf --text 'Hello' -o hello.png --scale 4
fontpixel batch jobs.toml
```

Every command prints a report of repairs and losses. `--strict` makes any
information loss an error and leaves the destination untouched; outputs are
validated in memory before they replace existing files. `fontpixel <command>
--help` documents every option.

The default build includes all 99 formats. A smaller binary:
`cargo install fontpixel-cli --no-default-features --features bdf,pcf`.

Documentation: the [CLI guide](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/w6.1-cli.md)
and the [format documents](https://github.com/fontpixel/fontpixel-rs/tree/main/docs/formats).
Library: [`fontpixel`](https://crates.io/crates/fontpixel).

## License

MIT
