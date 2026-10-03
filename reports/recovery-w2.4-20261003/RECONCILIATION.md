# New recovery reconciliation: isolated fuzz duplicate policy

This is a newly applied maintenance fix, not an exact transcription of a lost old file. The first fresh isolated fuzz dependency audit failed because fuzz/deny.toml lacked the root policy's already-recorded exact miniz_oxide 0.8.9 duplicate exception. The original W2.4 final fuzz audit was never observed after e08d69c.

On 2026-10-03 the coordinator explicitly approved applying only the matching exact-version exception after verifying the graph. cargo tree confirms 0.8.9 enters through png 0.18.1, while 0.9.1 enters through flate2 1.1.10. The root skip rule is copied unchanged. No dependency downgrade, advisory ignore, license allowance or broad duplicate relaxation is added. Production source is unchanged.

The initial failure is preserved in fuzz-deny-before-reconciliation.log/.exit. The post-change audit is fuzz-deny.log/.exit; the root policy is independently rerun in root-deny.log/.exit.
