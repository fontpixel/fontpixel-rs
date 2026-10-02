Reserved for reproducible external-tool crosschecks. No font-codec crosscheck is implemented or claimed in W0.1.

W1.2 adds `bdf.py`: all 59 supplied BDF fonts are compared byte-for-byte and
warning-for-warning with Python, plus bidirectional monobit pixels/metrics
(including unencoded glyphs). Generated files use a temporary directory. Exact
results, corpus hashes and external monobit tag differences are in
`bdf-results.json`; see docs/development.md for invocation and dependencies.
