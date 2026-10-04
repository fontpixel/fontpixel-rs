# Ordinary shared-path review (reconstructed history)

This document reconstructs findings and owner responses from retained
conversation context. The original Phase 6 sources/logs were lost. No shared
implementation is restored here, no current fix is certified, and no code was
executed during recovery. Other workers/coordinator own their reconstruction.

## Findings and historical disposition

1. Charset alias deduplication used Vec::contains per mapping, causing O(A^2)
   work. The owner changed it to ordered-set membership preserving first mapping
   order, with alias workspace checks.
2. Unmappable default-character metadata was silently set to None. The owner
   added charset.unmappable_default_character as an explicit loss, so strict
   mode can reject it.
3. Merely reducing sfnt ResourceLimits.max_total_bytes did not constrain cmap
   map expansion: max_cmap_mappings stayed 0x110000. The owner added explicit
   memory-derived table/name/cmap/glyph/CFF limits and container/face allowances.
   An initially overconservative per-point divisor was replaced by layout-based
   glyph/point/contour/component/instruction/scratch reservations. Large-Tong
   validation was requested; its final outcome is not retained here.
4. Default sfnt validation bypassed native reading and omitted CBLC/CBDT/sbix.
   The owner added those supported color-table checks and revised scope wording.
   It remains structural validation, not universal OpenType/OTS certification.
5. convert --each reduced later reader limits but reset writer limits to full.
   Parent changed the common writer allowance to subtract retained earlier
   output, counting byte capacities and conservative retained JSON storage.
6. Charset edit_glyph deep-clones the current bitmap/metadata. The owner added
   one conservative borrowed Font.retained_usage pass per strike for clone,
   ID-list and alias workspace, plus diagnostic reservation. This avoids an
   O(N^2) per-glyph full-font accounting pass.
7. Bounded serialized task JSON was decoded into retained serde_json Value
   trees and later expanded into another aggregate tree. The owner switched to
   boxed RawValue task results/diagnostics and typed aggregate serialization.
8. Merge multi-output staging initially charged only asset byte lengths,
   omitting report/path/capacity storage. Its owner reported exact typed report
   accounting, byte capacities and staging reserves. The borrowed merged model
   was charged twice by ledger plus native writer; the owner reported restoring
   exactly its reservation for writer invocation while retaining reduced
   output and cumulative ledgers.
9. W6.6 discovered and this worker corroborated that NativeWriter::write ignored
   common limits before plain native serialization; only gzip replay restricted
   the codec. Parent later reported implementing FontSet validation plus
   intersection/restricted-codec admission before a single native write, with
   four new phase6_native_budget tests for BDF glyph-zero, byte caps and no
   enlargement. These tests were reported as added; their final run outcome is
   not available here. BDF specifically needs model preflight because its older
   typed writer options only carry output-byte limits.

## Transaction and input protection review

Parent's reverse-drop Transactions wrapper was reviewed before loss. Later
bundle groups dropped before the group that created their shared supporting
directory, allowing rollback to remove new directories after staged/installed
files were cleaned. Current local staging transactions also drop before prior
retained groups on error. No concrete defect was found in this fix by source
review. A regression at a supporting-file installation boundary was present;
looping all failure boundaries was suggested.

The batch owner added declared input protection before dispatch, including tasks
that fail arguments and missing inputs. Canonical existing ancestors plus
missing tails allow lexical input/output ancestor and descendant protection,
complementing inode alias protection. The output check runs before persistence.
No fresh execution or current recovered-source verification is claimed here.

## Source-identity requirement

These changes touch shared resources, registry/transport, model and CLI paths.
The earlier PFR focused run cannot stand in for final shared-source integration
validation, and old long campaigns cannot establish their sanitizer coverage.
After reconstruction/integration, take new source snapshots, rerun affected
ordinary gates, and complete the user-deferred long campaigns locally. Keep
historical observations separate from newly obtained evidence.
