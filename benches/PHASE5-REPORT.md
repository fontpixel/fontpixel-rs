# Phase 5 native-format performance

Formal results for **185 cases and 301 engine/format records**, with five independent process repetitions per configured engine. All configured operations passed; no source, fixture, binary or oracle drift was found.

Engine records: {'rust': 197, 'python': 94, 'java': 6, 'native': 4}. Missing baselines remain unavailable, and no automatic speed ratios are claimed.

## Machine and interpretation

CPU: 12th Gen Intel(R) Core(TM) i7-12700H; 14 physical / 20 logical CPUs; RAM: 65503764 kB. Rust 1.99.0, release profile, thin LTO, one codegen unit. Full machine/compiler identity is in the raw JSON.

Coordinator confirmed all task-owned compilers, tests, fuzzers and external consumers stopped after final source gates and the v3 raw-log integrity fix. Unrelated desktop applications remained running on this shared workstation; no system-wide isolation or fixed CPU frequency is claimed. Earlier v2 measurements are superseded because dotted case IDs reused raw output paths.

Times below are microseconds per complete read/write operation, shown as **median [minimum, maximum]** across five process samples. Input bytes are preloaded; native output readback and budget auditing remain part of the writer where required. Counts retain each frozen adapter's actual loops. Peak RSS is whole-process KiB, including interpreter/runtime and setup; it is not isolated codec allocation. Rows emitted by the same process share its RSS measurement.

Each case is an original, validated native fixture/profile. Some writers use a different source-model workload from their reader fixture. Geometry, glyph counts, native padding and independent-tool projections must be checked before comparing engines. Detailed contracts, warmup counts and unsupported baselines are in [phase5-README.md](phase5-README.md); the raw result retains per-row notes, counts, fixture hashes and all five observations.

These phase 5 native cases do **not** establish the complete Tong export, catalogue batch or binding performance goals in PLAN §10. Those remain stage 6–8 work. PFR shared-physical-table amplification remains bounded by resource limits and documented in [the scaling review](../reports/w5.scaling-review.md); the ordinary PFR fixtures here do not measure that adversarial scaling family.

The earlier v2 quiet run is superseded: extension-bearing case IDs collided in raw log filenames. The fixed v3 runner appends stream suffixes, rejects existing log paths and verifies every raw hash before publication. Original rejected evidence remains under `reports/phase5-validation/superseded/performance-v2/`; none of its timings are used below.

## All measurements

| Case / returned format | Engine | Adapter glyphs | Read/write loops | Read µs | Write µs | Peak RSS KiB |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| a1:dosstart / dosstart | rust | 96 | 100/100 | 55.544 [36.046, 57.461] | 118.724 [115.665, 154.261] | 28004 |
| a1:dosstart / dosstart | python | — | 100/100 | 4862.407 [4760.770, 5165.411] | 11172.532 [10794.624, 11276.479] | 37136 |
| a1:edwin / edwin | rust | 4 | 100/100 | 3.913 [3.554, 4.085] | 10.640 [6.669, 11.551] | 28004 |
| a1:edwin / edwin | python | — | 100/100 | 389.698 [373.129, 399.015] | 761.129 [756.655, 764.847] | 36812 |
| a1:mkwinfont / mkwinfont | rust | 4 | 100/100 | 3.510 [3.239, 3.552] | 10.298 [9.804, 10.438] | 28004 |
| a1:mkwinfont / mkwinfont | python | — | 100/100 | 526.618 [511.978, 542.753] | 1086.709 [1080.428, 1170.890] | 34984 |
| a1:psf2txt / psf2txt | rust | 4 | 100/100 | 5.266 [5.195, 5.408] | 12.390 [11.957, 12.768] | 28004 |
| a1:psf2txt / psf2txt | python | — | 100/100 | 506.577 [486.621, 532.345] | 450.245 [426.364, 465.403] | 35776 |
| a1:pcr / pcr | rust | 256 | 100/100 | 55.893 [34.707, 62.094] | 89.777 [84.311, 111.677] | 28004 |
| a1:pcr / pcr | python | — | 100/100 | 6554.817 [6169.288, 6650.505] | 24759.587 [23392.990, 25131.600] | 66348 |
| a1:plus3dos / plus3dos | rust | 96 | 100/100 | 20.626 [16.636, 21.770] | 44.207 [28.024, 47.823] | 28004 |
| a1:plus3dos / plus3dos | python | — | 100/100 | 4161.224 [3910.605, 4191.414] | 6396.961 [6085.667, 6426.068] | 47372 |
| a1:drhalo / drhalo | rust | 4 | 100/100 | 0.962 [0.953, 1.076] | 2.305 [2.277, 2.400] | 28004 |
| a1:drhalo / drhalo | python | — | 100/— | 590.713 [584.992, 616.169] | — | 35776 |
| a1:writeon / writeon | rust | 96 | 100/100 | 20.669 [11.815, 26.926] | 39.886 [31.276, 45.450] | 28004 |
| a1:writeon / writeon | python | — | 100/100 | 4773.944 [4545.683, 4843.147] | 10204.104 [9793.124, 10261.369] | 55892 |
| a2:bbc / bbc | rust | 3 | 100/100 | 0.673 [0.641, 0.885] | 1.739 [1.713, 1.998] | 28004 |
| a2:bbc / bbc | python | 3 | 100/100 | 281.633 [263.242, 285.106] | 790.674 [745.258, 794.459] | 39352 |
| a2:wyse / wyse | rust | 3 | 100/100 | 0.774 [0.466, 0.817] | 2.080 [1.317, 2.125] | 28004 |
| a2:wyse / wyse | python | 3 | 100/100 | 394.094 [389.828, 396.075] | 321.911 [319.691, 322.395] | 39544 |
| a2:grasp / grasp | rust | 3 | 100/100 | 0.596 [0.390, 0.643] | 1.664 [1.277, 1.725] | 28004 |
| a2:grasp / grasp | python | 3 | 100/100 | 298.926 [296.002, 319.345] | 597.797 [562.472, 607.008] | 39880 |
| a2:hrcg / hrcg | rust | 96 | 100/100 | 54.355 [31.580, 55.635] | 72.337 [63.937, 98.518] | 28004 |
| a2:hrcg / hrcg | python | 96 | 100/100 | 6047.390 [5861.239, 6081.394] | 21629.076 [20842.102, 21786.238] | 80824 |
| a2:xbin / xbin | rust | 256 | 100/100 | 55.284 [34.141, 59.976] | 91.016 [79.024, 118.721] | 28004 |
| a2:xbin / xbin | python | 256 | 100/100 | 5149.047 [5116.904, 5337.924] | 6859.720 [6686.588, 6990.609] | 39900 |
| a2:xbin-512 / xbin-512 | rust | 512 | 100/100 | 79.642 [70.829, 117.785] | 171.574 [165.116, 193.484] | 28004 |
| a2:xbin-512 / xbin-512 | python | 512 | 100/— | 10243.508 [10161.913, 10660.859] | — | 40192 |
| a2:consoleet / consoleet | rust | 3 | 100/100 | 2.005 [1.973, 2.111] | 5.542 [5.354, 5.900] | 28004 |
| a2:consoleet / consoleet | python | 3 | 100/100 | 132.551 [128.631, 139.079] | 225.265 [217.164, 237.025] | 39632 |
| a2:zapfont-old / zapfont-old | rust | 3 | 100/100 | 0.709 [0.429, 0.768] | 1.786 [1.272, 1.864] | 28004 |
| a2:zapfont-old / zapfont-old | python | 3 | 100/100 | 503.623 [483.604, 517.577] | 1003.674 [964.991, 1011.676] | 41608 |
| a2:zapfont-redraw / zapfont-redraw | rust | 257 | 100/100 | 96.698 [91.106, 114.245] | 150.432 [130.455, 151.359] | 28004 |
| a2:zapfont-redraw / zapfont-redraw | python | 257 | 100/— | 21748.786 [21655.368, 21763.110] | — | 72680 |
| a2:comfont-fontedit / comfont-fontedit | rust | 256 | 100/100 | 38.020 [33.663, 44.654] | 83.391 [82.840, 86.662] | 28004 |
| a2:comfont-fontedit / comfont-fontedit | python | 256 | 100/— | 5358.106 [5333.261, 5390.693] | — | 39560 |
| a2:comfont-frapt / comfont-frapt | rust | 256 | 100/100 | 40.177 [34.544, 44.815] | 87.201 [81.291, 88.882] | 28004 |
| a2:comfont-frapt / comfont-frapt | python | 256 | 100/— | 5336.522 [5324.451, 5367.054] | — | 39576 |
| a2:comfont-frapt-tsr / comfont-frapt-tsr | rust | 256 | 100/100 | 32.831 [30.975, 53.778] | 86.035 [78.512, 87.428] | 28004 |
| a2:comfont-frapt-tsr / comfont-frapt-tsr | python | 256 | 100/— | 5405.370 [5369.678, 5573.460] | — | 39476 |
| a2:comfont-mania / comfont-mania | rust | 256 | 100/100 | 39.971 [32.666, 51.355] | 80.148 [77.860, 87.489] | 28004 |
| a2:comfont-mania / comfont-mania | python | 256 | 100/— | 5466.526 [5233.303, 6034.474] | — | 39512 |
| a2:comfont-letafont-pcw / comfont-letafont-pcw | rust | 256 | 100/100 | 34.273 [32.922, 56.706] | 82.394 [74.650, 87.265] | 28004 |
| a2:comfont-letafont-pcw / comfont-letafont-pcw | python | 256 | 100/— | 11322.967 [10560.999, 11675.144] | — | 40612 |
| a2:comfont-udg-pcw / comfont-udg-pcw | rust | 256 | 100/100 | 43.434 [34.531, 66.264] | 95.055 [88.840, 132.588] | 28004 |
| a2:comfont-udg-pcw / comfont-udg-pcw | python | 256 | 100/— | 10689.960 [10325.695, 10995.683] | — | 40632 |
| a2:comfont-psfcom-pcw / comfont-psfcom-pcw | rust | 256 | 100/100 | 54.424 [32.900, 57.846] | 86.288 [75.475, 129.677] | 28004 |
| a2:comfont-psfcom-pcw / comfont-psfcom-pcw | python | 256 | 100/— | 10166.771 [10090.000, 10388.204] | — | 40588 |
| a2:comfont-psfcom-spectrum-512-16 / comfont-psfcom-spectrum-512-16 | rust | 512 | 100/100 | 90.602 [84.117, 113.719] | 207.555 [196.345, 214.886] | 28004 |
| a2:comfont-psfcom-spectrum-512-16 / comfont-psfcom-spectrum-512-16 | python | 512 | 100/— | 25026.489 [24307.648, 25216.164] | — | 42276 |
| c1:daisy / daisy | rust | 91 | 300/300 | 32.551 [30.688, 34.112] | 58.683 [53.600, 60.576] | 28004 |
| c1:daisy / daisy | python | 91 | 100/— | 6138.826 [6096.847, 6471.192] | — | 39592 |
| c1:polyprint / polyprint | rust | 144 | 300/300 | 113.738 [108.403, 127.939] | 247.683 [233.151, 270.308] | 28004 |
| c1:polyprint / polyprint | python | 144 | 100/— | 36398.735 [34325.179, 38705.100] | — | 42052 |
| c1:printshop / printshop | rust | 95 | 300/300 | 13.608 [12.745, 14.696] | 28.162 [26.506, 28.931] | 28004 |
| c1:printshop / printshop | python | 95 | 100/— | 1904.746 [1857.658, 1939.982] | — | 39096 |
| c1:tasprint / tasprint | rust | 96 | 300/300 | 26.766 [26.139, 28.171] | 64.117 [61.633, 65.952] | 28004 |
| c1:tasprint / tasprint | python | 96 | 100/100 | 7411.157 [7099.471, 7560.215] | 15747.924 [14998.865, 16096.420] | 55172 |
| c1:pxl / pxl | rust | 128 | 300/300 | 52.942 [46.192, 63.313] | 80.028 [74.622, 84.305] | 28004 |
| c1:pxl / pxl | python | 128 | 100/— | 16430.415 [15923.786, 16793.166] | — | 39392 |
| c1:signum / signum | rust | 127 | 300/300 | 65.345 [54.210, 72.696] | 91.697 [85.113, 94.129] | 28004 |
| c1:signum / signum | python | 127 | 100/— | 6662.282 [6391.385, 6889.296] | — | 54872 |
| c1:pxl-1002 / pxl | rust | 128 | 300/300 | 54.017 [46.589, 58.878] | 77.254 [72.828, 84.798] | 28004 |
| c1:pxl-1002 / pxl | python | 128 | 100/— | 38774.126 [38474.207, 38969.620] | — | 39456 |
| c1:signum-p24 / signum | rust | 127 | 300/300 | 56.053 [51.017, 63.078] | 88.370 [86.558, 89.468] | 28004 |
| c1:signum-p24 / signum | python | 127 | 100/— | 5634.868 [5402.336, 5694.287] | — | 39388 |
| e1:read-rb11 / rb11 | rust | 3 | 100/— | 1.293 [1.267, 1.812] | — | 28004 |
| e1:read-rb12 / rb12 | rust | 3 | 100/— | 1.178 [1.154, 1.243] | — | 28004 |
| e1:read-u8m / u8m | rust | 4 | 100/— | 1.859 [1.802, 1.914] | — | 28004 |
| e1:read-cybiko / cybiko | rust | 3 | 100/— | 0.913 [0.856, 1.065] | — | 28004 |
| e1:read-hmzk / hmzk | rust | 3 | 100/— | 1.182 [0.876, 1.399] | — | 28004 |
| e1:read-sbf / sbf | rust | 3 | 100/— | 2.825 [1.709, 3.375] | — | 28004 |
| e1:read-playdate / playdate | rust | 5 | 100/— | 13.619 [9.172, 15.739] | — | 28004 |
| e1:read-split / split | rust | 5 | 100/— | 9.829 [6.180, 12.931] | — | 28004 |
| e1:read-pft / pft | rust | 5 | 100/— | 7.120 [4.844, 7.448] | — | 28004 |
| e1:writers / rb12 | rust | 256 | —/100 | — | 238.861 [215.930, 289.079] | 28004 |
| e1:writers / u8m | rust | 256 | —/100 | — | 225.026 [200.383, 245.195] | 28004 |
| e1:writers / hmzk | rust | 256 | —/100 | — | 217.374 [190.040, 366.783] | 28004 |
| e1:writers / cybiko | rust | 190 | —/100 | — | 162.783 [148.281, 174.679] | 28004 |
| e1:writers / sbf | rust | 190 | —/100 | — | 132.003 [121.967, 149.525] | 28004 |
| e1:writers / playdate | rust | 256 | —/100 | — | 524.916 [482.999, 542.988] | 28004 |
| e1:writers / pft | rust | 256 | —/100 | — | 195.978 [185.357, 204.304] | 28004 |
| x:legacy / raw | rust | 8 | 100/100 | 2.094 [1.951, 2.334] | 3.398 [3.209, 3.720] | 28004 |
| x:legacy / raw | python | — | 100/100 | 434.779 [417.901, 519.520] | 340.702 [323.645, 363.881] | 34992 |
| x:bits9 / raw | rust | 7 | 100/100 | 1.816 [1.806, 1.869] | 2.763 [2.757, 3.617] | 28004 |
| x:bits9 / raw | python | — | 100/100 | 328.955 [326.440, 341.776] | 219.813 [217.391, 247.007] | 35080 |
| x:columns16 / raw | rust | 6 | 100/100 | 1.509 [1.456, 1.599] | 2.320 [2.284, 2.456] | 28004 |
| x:columns16 / raw | python | — | 100/100 | 127.449 [120.916, 132.747] | 294.853 [285.603, 297.303] | 35096 |
| x:harlekin512 / raw | rust | 512 | 100/100 | 82.498 [76.955, 128.319] | 112.935 [109.540, 172.953] | 28004 |
| x:harlekin512 / raw | python | — | 100/100 | 6911.639 [6555.193, 6917.144] | 3138.670 [3105.906, 3239.484] | 36744 |
| x:bmfont / bmfont | rust | 3 | 100/100 | 31.933 [19.527, 32.964] | 213.070 [198.440, 214.930] | 28004 |
| x:bmfont / bmfont | python | — | 100/100 | 1058.930 [1042.174, 1112.118] | 1866.506 [1838.440, 1975.467] | 42836 |
| p3_52:hanfont-components / components | rust | 360 | 100/100 | 37.394 [34.718, 43.214] | 2043.284 [1942.360, 2092.271] | 28004 |
| p3_52:hanfont-components / components | python | 360 | 100/100 | 11917.016 [11532.146, 12077.677] | 11870.621 [11366.456, 11941.212] | 40428 |
| p3_52:hanfont-syllables / syllables | rust | 11172 | 100/100 | 2039.178 [1893.774, 2067.307] | 3266.162 [3234.424, 3502.220] | 28004 |
| p3_52:iigs-original / iigs | rust | 3 | 100/100 | 2.002 [1.945, 2.197] | 4.596 [4.502, 5.075] | 28004 |
| p3_52:iigs-original / iigs | python | 3 | 100/100 | 873.792 [871.986, 923.392] | 3706.248 [3682.228, 3880.565] | 40988 |
| p3_52:iigs-version105 / iigs | rust | 3 | 100/100 | 2.118 [1.935, 2.306] | 4.590 [4.543, 4.950] | 28004 |
| p3_52:iigs-version105 / iigs | python | 3 | 100/100 | 929.093 [887.346, 1017.499] | 4003.675 [3804.177, 4118.391] | 40900 |
| p3_52:iigs-binscii-0 / iigs | rust | 3 | 100/100 | 2.583 [1.819, 2.614] | 4.160 [2.858, 4.335] | 28004 |
| p3_52:iigs-binscii-0 / iigs | python | 3 | 100/— | 1614.582 [1570.351, 1697.050] | — | 40052 |
| p3_53:vfont / vfont | rust | 256 | 1000/1000 | 38.198 [36.517, 40.149] | 67.483 [65.026, 69.176] | 28004 |
| p3_53:vfont / vfont | python | 256 | 100/100 | 12814.969 [12265.147, 12999.193] | 11445.122 [11086.688, 11568.045] | 39972 |
| p3_53:fzx / fzx | rust | 224 | 1000/1000 | 30.699 [29.561, 31.592] | 72.239 [70.923, 75.370] | 28004 |
| p3_53:fzx / fzx | python | 224 | 100/100 | 13604.096 [13435.007, 14407.419] | 15917.458 [15190.011, 16199.204] | 42788 |
| p3_53:figlet / figlet | rust | 358 | 1000/1000 | 189.942 [186.775, 200.812] | 436.525 [423.486, 442.112] | 28004 |
| p3_53:figlet / figlet | python | 358 | 100/100 | 7698.386 [7513.337, 7935.381] | 32486.435 [32191.277, 33763.661] | 91480 |
| p3_54:palm / palm | rust | — | 400/400 | 60.226 [55.452, 61.728] | 114.536 [113.866, 116.603] | 28004 |
| p3_54:palm / palm | python | 257 | 100/— | 19095.401 [18998.651, 20132.192] | — | 41032 |
| p3_54:gdr / gdr | rust | — | 400/400 | 465.110 [454.284, 479.797] | 669.479 [658.160, 694.658] | 28004 |
| c:drcs / drcs | rust | 3 | 100/100 | 1.758 [1.245, 1.986] | 4.335 [2.853, 6.287] | 28004 |
| c:drcs / drcs | python | 3 | 100/100 | 477.633 [439.705, 497.700] | 4706.065 [4674.707, 5087.622] | 40988 |
| c:u8g2 / u8g2 | rust | 4 | 100/100 | 9.834 [6.307, 10.113] | 20.865 [12.810, 21.593] | 28004 |
| c:u8glib / u8glib | rust | 3 | 100/100 | 4.447 [4.358, 4.505] | 9.112 [5.947, 9.313] | 28004 |
| c:u8glib-compact / u8glib-compact | rust | 3 | 100/100 | 4.251 [4.248, 4.402] | 8.538 [6.621, 9.077] | 28004 |
| c:u8glib-gray / u8glib-gray | rust | 3 | 100/100 | 5.872 [5.711, 6.985] | 11.427 [7.245, 13.689] | 28004 |
| c:u8x8 / u8x8 | rust | 3 | 100/100 | 3.605 [3.394, 3.690] | 7.233 [6.982, 7.327] | 28004 |
| c:u8x8-legacy / u8x8-legacy | rust | 3 | 100/100 | 3.377 [2.130, 3.515] | 6.739 [4.121, 6.768] | 28004 |
| c:netbsd / netbsd | rust | 3 | 100/100 | 5.474 [4.760, 6.608] | 9.699 [8.560, 12.464] | 28004 |
| c:netbsd / netbsd | python | 3 | 100/100 | 539.556 [527.767, 554.587] | 1109.492 [1085.640, 1152.331] | 41136 |
| c:lvgl / lvgl | rust | 3 | 100/100 | 16.697 [16.461, 17.513] | 22.055 [14.684, 22.585] | 28004 |
| c:lvgl-rle / lvgl-rle | rust | 3 | 100/100 | 18.389 [11.406, 23.453] | 16.762 [15.473, 26.379] | 28004 |
| c:lvgl-xor / lvgl-xor | rust | 3 | 100/100 | 20.331 [17.231, 21.544] | 16.034 [15.233, 24.456] | 28004 |
| c:lvgl2-rle / lvgl2-rle | rust | 3 | 100/100 | 18.388 [13.842, 25.840] | 20.610 [15.934, 23.311] | 28004 |
| c:lvgl4-xor / lvgl4-xor | rust | 3 | 100/100 | 29.106 [23.310, 37.423] | 31.007 [21.556, 38.838] | 28004 |
| c:lvgl8 / lvgl8 | rust | 3 | 100/100 | 27.630 [23.562, 28.476] | 30.992 [26.225, 38.901] | 28004 |
| c:lvgl4-binary-xor / lvgl4-binary-xor | rust | 3 | 100/100 | 30.503 [20.179, 30.660] | 43.847 [32.733, 56.057] | 28004 |
| color:cbdt-sbix / Cbdt | rust | 4096 | 1/1 | 8803.618 [8299.762, 9751.071] | 38951.639 [35704.657, 43214.367] | 28004 |
| color:cbdt-sbix / Sbix | rust | 4096 | 1/1 | 7720.470 [7574.568, 8345.490] | 32589.011 [31478.891, 35410.397] | 28004 |
| d:fbit-256-time / fbit | rust | 256 | 100/100 | 140.805 [101.489, 160.100] | 231.332 [223.896, 260.067] | 28004 |
| d:fbit-256-time / fbit | python | 256 | 100/— | 7957.520 [7568.925, 8583.618] | — | 39756 |
| d:fbit-256-time-canonical / fbit | rust | 256 | 100/100 | 125.614 [110.907, 217.038] | 235.031 [226.489, 236.696] | 28004 |
| d:fbit-256-time-canonical / fbit | python | 256 | 100/— | 7873.149 [7586.581, 7962.348] | — | 39860 |
| d:lisa-256-time / lisa | rust | 256 | 100/100 | 73.480 [49.945, 74.409] | 115.836 [110.453, 158.382] | 28004 |
| d:lisa-256-time / lisa | python | 256 | 100/— | 17240.813 [16572.779, 17580.592] | — | 41008 |
| d:lisa-256-time-canonical / lisa | rust | 256 | 100/100 | 61.220 [45.145, 73.506] | 81.432 [74.662, 101.676] | 28004 |
| d:lisa-256-time-canonical / lisa | python | 256 | 100/— | 16759.152 [16566.873, 17500.523] | — | 40900 |
| d:mgtk-256-time / mgtk | rust | 256 | 100/100 | 118.066 [112.355, 142.178] | 250.097 [228.315, 277.146] | 28004 |
| d:mgtk-256-time / mgtk | python | 256 | 100/— | 21881.097 [21756.579, 22815.285] | — | 41264 |
| d:mgtk-256-time-canonical / mgtk | rust | 256 | 100/100 | 127.239 [106.199, 171.400] | 228.101 [218.469, 232.499] | 28004 |
| d:mgtk-256-time-canonical / mgtk | python | 256 | 100/— | 21868.420 [21618.098, 22809.911] | — | 41300 |
| d:qpf-256-time / qpf | rust | 256 | 100/100 | 60.948 [54.860, 74.931] | 105.594 [102.194, 119.948] | 28004 |
| d:qpf-256-time-canonical / qpf | rust | 256 | 100/100 | 65.789 [47.052, 82.679] | 72.812 [67.026, 92.227] | 28004 |
| d:qpf-2048-time / qpf | rust | 2048 | 100/100 | 422.320 [404.543, 439.741] | 999.100 [955.936, 1013.334] | 28004 |
| d:qpf-2048-time-canonical / qpf | rust | 2048 | 100/100 | 429.514 [424.342, 445.176] | 675.537 [633.162, 704.819] | 28004 |
| d:qpf2-256-time / qpf2 | rust | 256 | 100/100 | 72.378 [57.037, 96.698] | 135.819 [126.479, 142.150] | 28004 |
| d:qpf2-256-time-canonical / qpf2 | rust | 256 | 100/100 | 70.522 [66.172, 75.958] | 84.358 [79.204, 90.410] | 28004 |
| d:qpf2-2048-time / qpf2 | rust | 2048 | 100/100 | 589.405 [566.168, 602.138] | 1352.565 [1335.736, 1407.556] | 28004 |
| d:qpf2-2048-time-canonical / qpf2 | rust | 2048 | 100/100 | 599.850 [594.634, 633.109] | 847.737 [810.470, 908.062] | 28004 |
| d:binhex-time / binhex | rust | 3 | 100/100 | 6.917 [4.217, 7.854] | 9.653 [6.250, 10.658] | 28004 |
| d:binhex-time / binhex | python | 3 | 100/— | 3163.496 [3139.483, 3290.943] | — | 40168 |
| d:binhex-time-canonical / binhex | rust | 3 | 100/100 | 6.972 [6.821, 7.593] | 12.626 [12.211, 13.979] | 28004 |
| d:binhex-time-canonical / binhex | python | 3 | 100/— | 3263.644 [3242.457, 3291.050] | — | 40524 |
| b:beos / beos | rust | — | 100/20 | 237.067 [231.297, 286.490] | 485.390 [467.573, 519.433] | 28004 |
| b:beos-16 / beos-16 | rust | — | 100/20 | 2.498 [2.407, 2.601] | 5.710 [5.396, 5.991] | 28004 |
| b:beos-16 / beos-16 | python | 3 | 100/100 | 409.013 [408.344, 413.399] | 449.422 [448.260, 453.368] | 40176 |
| b:oberon / oberon | rust | — | 100/20 | 53.225 [49.461, 81.730] | 96.152 [87.915, 145.820] | 28004 |
| b:oberon / oberon | python | 224 | 100/100 | 10863.088 [10399.346, 11339.322] | 23159.189 [22366.529, 24331.159] | 65460 |
| b:symbos / symbos | rust | — | 100/20 | 34.861 [30.647, 35.562] | 64.800 [41.477, 70.379] | 28004 |
| b:symbos / symbos | python | 96 | 100/100 | 3746.936 [3723.611, 3923.708] | 1133.345 [1125.209, 1185.916] | 40016 |
| b:symbos-small / symbos-small | rust | — | 100/20 | 35.057 [22.074, 35.980] | 44.537 [37.744, 64.175] | 28004 |
| b:symbos-big / symbos-big | rust | — | 100/20 | 22.563 [19.219, 25.729] | 32.181 [30.424, 51.895] | 28004 |
| b:symbos-big-extended / symbos-big-extended | rust | — | 100/20 | 70.192 [51.803, 76.696] | 94.766 [93.211, 158.233] | 28004 |
| b:symbos-small-extended / symbos-small-extended | rust | — | 100/20 | 53.715 [51.192, 66.200] | 105.486 [97.152, 115.479] | 28004 |
| b:wsfont / wsfont | rust | — | 100/20 | 54.715 [43.197, 61.061] | 90.376 [84.700, 98.763] | 28004 |
| b:wsfont / wsfont | python | 224 | 100/100 | 12592.912 [12174.714, 12699.013] | 41992.305 [41793.911, 42226.252] | 112752 |
| b:wsfont-rbit / wsfont-rbit | rust | — | 100/20 | 1.465 [0.889, 1.569] | 3.458 [2.141, 3.556] | 28004 |
| b:wsfont-rbyte / wsfont-rbyte | rust | — | 100/20 | 1.458 [0.853, 1.543] | 3.365 [2.159, 3.562] | 28004 |
| b:wsfont-rbit-rbyte / wsfont-rbit-rbyte | rust | — | 100/20 | 1.480 [1.467, 1.560] | 3.363 [3.321, 3.408] | 28004 |
| b:wsfont-alpha / wsfont-alpha | rust | — | 100/20 | 114.281 [102.417, 122.546] | 163.779 [155.157, 169.599] | 28004 |
| b:vtfont / vtfont | rust | — | 100/20 | 56.456 [49.819, 60.831] | 51.372 [49.300, 59.482] | 28004 |
| b:vtfont / vtfont | python | 33 | 100/— | 2295.489 [2273.749, 2359.732] | — | 39212 |
| b:geos-bare / geos-bare | rust | — | 100/20 | 29.020 [18.126, 31.732] | 48.695 [29.748, 50.873] | 28004 |
| b:geos-bare / geos-bare | python | 95 | 100/100 | 3726.650 [3590.203, 3908.238] | 7314.121 [7048.015, 7491.066] | 50628 |
| b:geos / geos | rust | — | 100/20 | 29.266 [18.414, 30.388] | 38.158 [33.487, 59.010] | 28004 |
| b:geos / geos | python | 95 | 100/100 | 5221.362 [5139.027, 5759.756] | 8888.572 [8716.293, 9678.446] | 50748 |
| b:geos-extended / geos-extended | rust | — | 100/20 | 29.192 [17.572, 30.172] | 30.489 [29.378, 52.271] | 28004 |
| b:geos-utf8 / geos-utf8 | rust | — | 100/20 | 39.234 [22.727, 44.564] | 51.316 [47.861, 83.080] | 28004 |
| b:geos-mega / geos-mega | rust | — | 100/20 | 31.450 [22.838, 32.235] | 44.925 [35.644, 57.237] | 28004 |
| b:geos-mega-utf8 / geos-mega-utf8 | rust | — | 100/20 | 44.159 [23.765, 55.074] | 80.919 [48.184, 85.775] | 28120 |
| b:gpi-fixed / gpi-fixed | rust | — | 100/20 | 1.574 [1.561, 1.842] | 4.683 [4.559, 4.723] | 28120 |
| b:gpi-prop / gpi-prop | rust | — | 100/20 | 1.691 [1.542, 1.956] | 5.072 [4.505, 5.548] | 28120 |
| b:gpi / gpi | rust | — | 100/20 | 56.804 [38.209, 83.964] | 94.888 [82.436, 144.103] | 28120 |
| b:gpi-ne / gpi-ne | rust | — | 100/20 | 1.814 [1.808, 1.890] | 5.800 [5.594, 6.062] | 28120 |
| b:gpi-lx / gpi-lx | rust | — | 100/20 | 59.472 [52.171, 74.702] | 98.371 [90.222, 164.228] | 28120 |
| b:pcgeos / pcgeos | rust | — | 100/20 | 73.824 [54.222, 91.844] | 102.772 [92.769, 161.875] | 28120 |
| b:pcgeos / pcgeos | python | 224 | 100/100 | 16149.545 [16117.100, 17039.947] | 12546.265 [12537.869, 12932.989] | 63896 |
| b:pcgeos-styled / pcgeos-styled | rust | — | 100/20 | 1.542 [1.023, 1.776] | 4.339 [2.858, 4.917] | 28120 |
| b:pcgeos-dbcs / pcgeos-dbcs | rust | — | 100/20 | 6.118 [6.082, 6.227] | 13.734 [13.405, 13.770] | 28120 |
| b:vtfont-rebuild / vtfont-rebuild | rust | — | 100/20 | 56.334 [47.773, 77.276] | 73.788 [72.657, 107.205] | 28120 |
| b:vtfont-rebuild / vtfont-rebuild | python | 33 | 100/100 | 2381.324 [2237.913, 2408.204] | 3788.113 [3719.946, 3818.514] | 43332 |
| c2:catalyst-small / catalyst | rust | 128 | 300/300 | 40.365 [39.564, 40.620] | 73.537 [73.279, 73.930] | 28120 |
| c2:dashen-v3 / dashen | rust | 2 | 300/300 | 2.491 [1.600, 2.637] | 2.584 [2.488, 3.748] | 28120 |
| c2:dashen-v3 / dashen | python | 2 | 100/— | 616.707 [612.372, 621.288] | — | 39768 |
| c2:hppcl-raw / hppcl | rust | 1 | 300/300 | 1.028 [0.654, 1.093] | 2.937 [1.711, 3.015] | 28120 |
| c2:hppcl-raw / hppcl | python | 1 | 100/100 | 428.120 [423.238, 429.584] | 735.307 [733.523, 764.775] | 39220 |
| c2:catalyst-dense / catalyst | rust | 128 | 300/300 | 60.609 [59.639, 62.056] | 97.233 [96.835, 97.379] | 28120 |
| c2:catalyst-dense / catalyst | python | 128 | 100/— | 4500.180 [4483.217, 4511.635] | — | 39136 |
| c2:dashen-dense / dashen | rust | 95 | 300/300 | 40.717 [39.700, 42.955] | 59.020 [58.461, 59.781] | 28120 |
| c2:dashen-dense / dashen | python | 95 | 100/— | 5833.364 [5775.994, 5896.314] | — | 40416 |
| c2:hppcl-dense / hppcl | rust | 95 | 300/300 | 37.478 [35.321, 37.895] | 72.054 [71.705, 74.342] | 28120 |
| c2:hppcl-dense / hppcl | python | 95 | 100/100 | 4761.315 [4721.487, 4800.596] | 2815.968 [2782.265, 2820.783] | 40408 |
| c2:prebuilt-dense / prebuilt | rust | 96 | 300/300 | 26.238 [23.021, 30.447] | 56.125 [51.349, 56.963] | 28120 |
| c2:prebuilt-dense / prebuilt | python | 96 | 100/— | 5624.596 [5616.421, 5684.780] | — | 40388 |
| c2:catalyst-data / catalyst:data | rust | 128 | 300/300 | 40.158 [38.413, 41.983] | 73.437 [70.899, 74.966] | 28120 |
| c2:chiwriter-v3 / chiwriter | rust | 3 | 300/300 | 1.549 [1.103, 1.691] | 3.514 [2.380, 3.624] | 28120 |
| c2:chiwriter-v3 / chiwriter | python | 3 | 100/— | 391.198 [387.828, 400.655] | — | 38748 |
| c2:chiwriter-v4 / chiwriter | rust | 3 | 300/300 | 1.545 [1.431, 1.571] | 3.508 [2.099, 3.615] | 28280 |
| c2:chiwriter-v4 / chiwriter | python | 3 | 100/— | 387.218 [382.318, 395.395] | — | 38760 |
| c2:chiwriter-dense / chiwriter | rust | 95 | 300/300 | 43.947 [42.016, 50.864] | 65.234 [61.674, 65.993] | 28440 |
| c2:chiwriter-dense / chiwriter | python | 95 | 100/— | 1876.496 [1871.471, 1935.866] | — | 39220 |
| c2:chiwriter-absolute-stride3 / chiwriter:absolute:stride3 | rust | 3 | 300/300 | 1.022 [0.942, 1.636] | 2.261 [2.052, 3.606] | 28440 |
| c2:chiwriter-relative-stride3 / chiwriter:relative:stride3 | rust | 3 | 300/300 | 1.661 [0.984, 2.057] | 2.570 [2.253, 4.725] | 28440 |
| c2:dashen-v4 / dashen | rust | 3 | 300/300 | 4.075 [2.499, 5.628] | 5.712 [4.345, 8.334] | 28440 |
| c2:hppcl-rle / hppcl | rust | 1 | 300/300 | 1.365 [0.802, 1.746] | 2.820 [2.152, 4.410] | 28440 |
| c2:hppcl-legacy / hppcl:legacy | rust | 1 | 300/300 | 1.368 [1.228, 1.446] | 3.407 [2.558, 4.067] | 28440 |
| c2:hppcl-legacy / hppcl:legacy | python | 1 | 100/100 | 432.945 [426.834, 448.181] | 748.348 [729.370, 762.599] | 39040 |
| c2:prebuilt-be-1 / prebuilt | rust | 1 | 300/300 | 1.067 [1.062, 1.117] | 2.843 [2.702, 2.885] | 28440 |
| c2:prebuilt-be-1 / prebuilt | python | 1 | 100/— | 474.524 [473.034, 494.948] | — | 39624 |
| c2:prebuilt-be-2 / prebuilt | rust | 1 | 300/300 | 1.070 [0.672, 1.084] | 2.756 [1.676, 2.824] | 28440 |
| c2:prebuilt-be-2 / prebuilt | python | 1 | 100/— | 492.066 [472.533, 498.904] | — | 39552 |
| c2:prebuilt-be-4 / prebuilt | rust | 1 | 300/300 | 1.085 [1.068, 1.112] | 2.817 [2.326, 2.889] | 28440 |
| c2:prebuilt-be-4 / prebuilt | python | 1 | 100/— | 481.321 [479.843, 503.206] | — | 39788 |
| c2:prebuilt-be-8 / prebuilt | rust | 1 | 300/300 | 1.078 [0.675, 1.237] | 2.851 [1.613, 3.075] | 28440 |
| c2:prebuilt-be-8 / prebuilt | python | 1 | 100/— | 504.363 [496.214, 515.890] | — | 39752 |
| c2:prebuilt-le-1 / prebuilt | rust | 1 | 300/300 | 1.088 [0.702, 1.265] | 2.780 [1.635, 2.903] | 28440 |
| c2:prebuilt-le-2 / prebuilt | rust | 1 | 300/300 | 1.064 [0.658, 1.207] | 2.810 [1.556, 2.891] | 28440 |
| c2:prebuilt-le-4 / prebuilt | rust | 1 | 300/300 | 1.095 [1.065, 1.098] | 2.808 [2.781, 2.848] | 28440 |
| c2:prebuilt-le-8 / prebuilt | rust | 1 | 300/300 | 1.745 [1.081, 1.794] | 2.810 [2.263, 5.113] | 28440 |
| c2:prebuilt-vertical / prebuilt | rust | 1 | 300/300 | 1.275 [0.787, 2.164] | 3.254 [1.921, 5.178] | 28440 |
| c2:prebuilt-vertical / prebuilt | python | 1 | 100/— | 522.406 [517.232, 528.628] | — | 39608 |
| c2:prebuilt-be-8-gray / prebuilt | rust | 1 | 300/300 | 1.058 [1.056, 1.133] | 2.838 [2.785, 2.865] | 28440 |
| c2:prebuilt-be-8-gray / prebuilt | python | 1 | 100/— | 516.106 [460.484, 524.404] | — | 39516 |
| c2:riscos-v4 / riscos | rust | 1 | 300/300 | 1.366 [0.930, 1.419] | 2.815 [1.632, 3.103] | 28440 |
| c2:riscos-v4 / riscos | python | 1 | 100/— | 160.804 [155.980, 163.218] | — | 39416 |
| c2:riscos-v5 / riscos | rust | 1 | 300/300 | 1.374 [0.851, 1.629] | 1.701 [1.630, 2.866] | 28600 |
| c2:riscos-v5 / riscos | python | 1 | 100/— | 157.482 [156.015, 164.419] | — | 39460 |
| c2:riscos-v6 / riscos | rust | 1 | 300/300 | 1.384 [1.360, 1.450] | 2.829 [2.808, 2.897] | 28600 |
| c2:riscos-v6 / riscos | python | 1 | 100/— | 160.393 [152.879, 164.034] | — | 39356 |
| c2:riscos-v7 / riscos | rust | 1 | 300/300 | 1.396 [1.370, 1.410] | 2.820 [2.809, 2.991] | 28600 |
| c2:riscos-v8 / riscos | rust | 1 | 300/300 | 1.390 [1.369, 1.513] | 3.321 [3.275, 3.336] | 28600 |
| c2:riscos-dense / riscos | rust | 95 | 300/300 | 25.178 [20.164, 31.448] | 40.797 [39.215, 43.034] | 28600 |
| c2:riscos-dense / riscos | python | 95 | 100/— | 4740.867 [4649.844, 4863.757] | — | 40268 |
| c2:riscos-packed / riscos | rust | 1 | 300/300 | 1.426 [1.400, 1.491] | 3.239 [3.206, 3.294] | 28600 |
| c2:riscos-packed / riscos | python | 1 | 100/— | 162.503 [155.428, 165.422] | — | 39328 |
| c2:riscos-phases / riscos | rust | 16 | 300/300 | 15.357 [13.267, 21.292] | 23.524 [21.400, 41.790] | 28600 |
| c2:riscos-wide / riscos | rust | 1 | 300/300 | 1.726 [1.026, 1.741] | 3.680 [2.820, 4.492] | 28600 |
| c2:riscos-xy / riscos | rust | 1 | 300/300 | 2.439 [2.016, 3.833] | 3.816 [2.542, 4.895] | 28600 |
| c2:riscos-xy / riscos | python | 1 | 100/— | 216.923 [214.636, 222.314] | — | 39480 |
| c2:riscos-xy-gray / riscos | rust | 1 | 300/300 | 1.601 [1.569, 1.808] | 2.437 [2.405, 2.751] | 28600 |
| c2:riscos-xy-gray / riscos | python | 1 | 100/— | 216.542 [214.928, 226.845] | — | 39360 |
| c2:xerox-alto / xerox:alto | rust | 1 | 300/300 | 1.731 [1.141, 1.735] | 5.637 [3.753, 5.925] | 28600 |
| c2:xerox-alto / xerox:alto | python | 1 | 100/— | 1244.233 [1200.664, 1251.644] | — | 38756 |
| c2:xerox-bitblt / xerox:bitblt | rust | 2 | 300/300 | 0.817 [0.790, 0.852] | 2.694 [2.431, 2.839] | 28760 |
| c2:xerox-bitblt / xerox:bitblt | python | 2 | 100/— | 329.735 [321.877, 341.593] | — | 38800 |
| c2:xerox-bitblt-dense / xerox:bitblt | rust | 96 | 300/300 | 19.432 [18.745, 28.435] | 35.680 [33.762, 36.817] | 28920 |
| c2:xerox-bitblt-dense / xerox:bitblt | python | 96 | 100/— | 1436.631 [1385.552, 1441.280] | — | 38868 |
| c2:xerox-prepress / xerox:prepress | rust | 1 | 300/300 | 1.072 [0.649, 1.159] | 2.708 [1.493, 2.831] | 28920 |
| c2:xerox-prepress / xerox:prepress | python | 1 | 100/— | 325.969 [320.269, 341.553] | — | 38520 |
| e2:read-pilfont / pilfont | rust | 100 | 100/— | 39.993 [28.713, 47.529] | — | 28920 |
| e2:read-pilfont / pilfont | python | 100 | 100/— | 24.923 [23.666, 25.161] | — | 34520 |
| e2:read-vlw8 / vlw8 | rust | 100 | 100/— | 22.057 [15.635, 24.766] | — | 28920 |
| e2:read-vlw8 / vlw8 | java | 100 | 100/— | 42.876 [40.353, 45.155] | — | 58520 |
| e2:read-vlw9 / vlw9 | rust | 100 | 100/— | 24.610 [21.428, 25.466] | — | 28920 |
| e2:read-vlw9 / vlw9 | java | 100 | 100/— | 45.944 [40.202, 48.997] | — | 57772 |
| e2:read-vlw10 / vlw10 | rust | 100 | 100/— | 26.530 [21.433, 40.620] | — | 29080 |
| e2:read-vlw10 / vlw10 | java | 100 | 100/— | 45.360 [43.915, 46.860] | — | 58604 |
| e2:read-vlw / vlw | rust | 100 | 100/— | 21.971 [19.462, 33.202] | — | 29080 |
| e2:read-vlw / vlw | java | 100 | 100/— | 47.737 [46.695, 50.211] | — | 57552 |
| e2:read-sfont / sfont | rust | 94 | 100/— | 68.184 [61.930, 75.761] | — | 29080 |
| e2:read-sfont / sfont | java | 95 | 100/— | 816.967 [783.468, 854.062] | — | 94620 |
| e2:read-rfont / rfont | rust | 100 | 100/— | 184.700 [175.258, 206.464] | — | 29080 |
| e2:read-rfont / rfont | java | 100 | 100/— | 1325.505 [1302.474, 1374.645] | — | 152464 |
| e2:read-imageset / imageset | rust | 100 | 100/— | 159.411 [142.775, 167.914] | — | 29080 |
| e2:read-imageset / imageset | python | 100 | 100/— | 9156.680 [9003.421, 9448.719] | — | 45744 |
| e2:image-writers / pilfont | rust | 100 | —/1 | — | 228.395 [215.944, 245.596] | 29080 |
| e2:image-writers / vlw | rust | 100 | —/1 | — | 151.374 [130.431, 158.099] | 29080 |
| e2:image-writers / sfont | rust | 94 | —/1 | — | 235.431 [216.388, 249.273] | 29080 |
| e2:image-writers / rfont | rust | 100 | —/1 | — | 680.950 [627.422, 715.715] | 29080 |
| e2:image-writers / imageset | rust | 100 | —/1 | — | 573.499 [370.581, 675.492] | 29080 |
| e2:ssfn-original.sfn / ssfn | rust | — | 100/100 | 1.203 [0.801, 1.304] | 3.017 [1.984, 3.075] | 29080 |
| e2:ssfn-original.asc / ssfn | rust | — | 100/100 | 2.420 [1.541, 2.685] | 3.337 [2.305, 4.025] | 29080 |
| e2:ssfn-gray.asc / ssfn | rust | — | 100/100 | 4.606 [3.943, 5.290] | 7.400 [4.715, 8.131] | 29080 |
| e2:ssfn-ligature.asc / ssfn | rust | — | 100/100 | 2.550 [1.778, 2.962] | 4.290 [2.561, 4.714] | 29240 |
| e2:ssfn-gray.sfn / ssfn | rust | — | 100/100 | 1.332 [0.827, 1.635] | 4.675 [3.481, 6.496] | 29240 |
| e2:ssfn-repeat.sfn / ssfn | rust | — | 100/100 | 1.363 [1.359, 1.462] | 5.841 [5.707, 6.079] | 29240 |
| e2:ssfn-collection.sfn / ssfn | rust | — | 100/100 | 2.812 [2.184, 2.866] | 6.399 [4.262, 7.107] | 29240 |
| e2:ssfn-gzip.sfn.gz / ssfn | rust | — | 100/100 | 9.128 [9.045, 9.927] | 3.090 [3.001, 3.156] | 29240 |
| e2:pfr-0-0-1.pfr / pfr | rust | — | 100/100 | 1.449 [1.423, 1.574] | 3.880 [3.664, 4.724] | 29240 |
| e2:pfr-1-0-1.pfr / pfr | rust | — | 100/100 | 1.954 [1.716, 2.389] | 5.227 [4.866, 6.854] | 29240 |
| e2:pfr-2-0-1.pfr / pfr | rust | — | 100/100 | 2.189 [1.508, 2.558] | 5.363 [3.489, 5.846] | 29240 |
| e2:pfr-ft-0-0-1.pfr / pfr | rust | — | 100/100 | 3.066 [2.543, 3.109] | 7.136 [4.327, 8.114] | 29240 |
| e2:pfr-ft-0-0-1.pfr / pfr | native | 1 | 100/— | 16.680 [16.138, 16.867] | — | 30532 |
| e2:pfr-ft-1-0-1.pfr / pfr | rust | — | 100/100 | 1.427 [1.006, 1.470] | 3.765 [2.122, 3.898] | 29240 |
| e2:pfr-ft-1-0-1.pfr / pfr | native | 1 | 100/— | 16.402 [15.276, 16.438] | — | 30524 |
| e2:pfr-ft-2-0-1.pfr / pfr | rust | — | 100/100 | 1.509 [0.969, 1.537] | 3.769 [2.207, 3.873] | 29240 |
| e2:pfr-ft-2-0-1.pfr / pfr | native | 1 | 100/— | 16.597 [15.527, 16.807] | — | 30484 |
| e2:pfr-ft-multistrike.pfr / pfr | rust | — | 100/100 | 2.909 [2.853, 3.047] | 7.971 [7.868, 8.310] | 29240 |
| e2:pfr-ft-multistrike.pfr / pfr | native | 2 | 100/— | 28.678 [26.296, 29.688] | — | 30420 |
| transport:raw-gzip / raw | rust | 256 | 100/100 | 44.346 [38.925, 66.340] | 65.746 [62.120, 68.613] | 29240 |
| transport:raw-gzip / gzip | rust | 256 | 100/100 | 61.226 [54.704, 61.532] | 242.521 [218.335, 254.916] | 29240 |

## Reproduction and archived evidence

Use the prepare/measure commands in [phase5-README.md](phase5-README.md). The accepted preparation is `prep-v3-final.json` and the measurement is `quiet-final3.json`. All source/fixture/oracle identities are verified by the runner before and after timing. Reproduction needs the documented external validation tool installations; they are not runtime dependencies of the CLI.

- [Complete result, gzip JSON](../reports/phase5-validation/quiet-final3.json.gz).
- [Preparation identity, gzip JSON](../reports/phase5-validation/prep-v3-final.json.gz).
- [Original stdout/stderr and progress records](../reports/phase5-validation/quiet-final3-raw.tar.gz). Absolute raw-output paths in JSON map to the archive's `quiet-final3/` directory.
- [Archive checksums and summary](../reports/phase5-validation/performance-summary.json).
