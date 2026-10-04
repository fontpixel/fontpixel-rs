These MIT project fixtures use original artwork and names. The Python source
`fuzz/seed_w5e2_outline.py` assembles SFN2/SFNC wire records independently from
production code, plus author-defined ASCII source with exact dimensions, alpha
coverage and a ligature. No upstream font artwork is included. External sfnconv
outputs remain under ../local-work; their differing terminal skips, optional
ligature terminator and ASCII alpha coalescing are explicitly recorded/asserted
by tests/crosscheck/w5e2_outline.py and the W5.E2-outline report.
