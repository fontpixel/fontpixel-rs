# Phase 6 final ASAN fuzz campaign

Label `phase6-final-20261005T040104Z` (gate 048), evaluated 2026-10-05T05:22:59.119041+00:00. Criterion: exit==0 and no crash-/timeout-/oom-/leak- artifacts and elapsed>=required and libFuzzer final stats present with executions>0

Limits: libFuzzer `-max_total_time=610`, `-rss_limit_mb=2048`, `-timeout=10`, 16 targets in parallel; AddressSanitizer with leak detection (defaults).

Result: 104/104 targets passed, 0 failed, 301,696,538 executions in total. Every one of the 99 reader IDs is reached by at least one target (see `fuzz-final.json` and the inventory in local-work).

| Target | Reader entry points | Also reaches | Profile | Elapsed (s) | Executions | cov | ft | Peak RSS (MB) | Exit |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| bdf | bdf |  | baseline | 612.1 | 262,433 | 3734 | 16320 | 583 | 0 |
| pcf | pcf |  | baseline | 611.6 | 1,020,044 | 4822 | 15572 | 485 | 0 |
| yaff | yaff |  | baseline | 611.3 | 1,247,382 | 5918 | 24760 | 535 | 0 |
| sfnt |  | ttf, otf | baseline | 611.6 | 2,613,786 | 3017 | 8767 | 497 | 0 |
| glyf |  | ttf | baseline | 611.6 | 1,925,042 | 1528 | 6667 | 494 | 0 |
| compat_ttf |  | ttf | baseline | 612.1 | 37,389 | 3588 | 13104 | 750 | 0 |
| cff |  | otf | baseline | 611.5 | 3,510,249 | 2824 | 8406 | 463 | 0 |
| embedded_bitmap | otb, bdat |  | embedded-only | 611.9 | 2,839,732 | 4776 | 11761 | 484 | 0 |
| color_bitmap | color |  | color-only | 611.6 | 182,705 | 6060 | 22431 | 495 | 0 |
| woff1 | woff |  | baseline | 611.3 | 5,887,218 | 998 | 3173 | 391 | 0 |
| woff2 | woff2 |  | baseline | 611.3 | 1,650,868 | 3633 | 12884 | 468 | 0 |
| gsub |  | ttf, otf | baseline | 611.4 | 3,688,456 | 2017 | 7564 | 580 | 0 |
| subset |  | ttf, otf | baseline | 611.8 | 3,153,871 | 6105 | 17243 | 496 | 0 |
| vector_raster | vector-raster, ttf, otf, woff, woff2, ttc |  | baseline | 611.5 | 375,842 | 10561 | 32953 | 590 | 0 |
| collection | ttc | woff, woff2 | baseline | 611.3 | 2,325,593 | 1076 | 2388 | 410 | 0 |
| svg | svg |  | baseline | 611.3 | 1,182,425 | 4643 | 17671 | 486 | 0 |
| hex | hex |  | baseline | 611.6 | 2,760,018 | 1041 | 5349 | 474 | 0 |
| psf | psf |  | baseline | 611.4 | 767,058 | 1209 | 5008 | 436 | 0 |
| fnt | fnt |  | baseline | 611.6 | 481,520 | 1678 | 5095 | 438 | 0 |
| fon | fon | fnt | fon-only | 612.0 | 10,552,416 | 685 | 1250 | 561 | 0 |
| image | image |  | image-c | 611.8 | 1,730,981 | 12135 | 38737 | 688 | 0 |
| c | c |  | c-only | 611.5 | 416,248 | 7455 | 28425 | 556 | 0 |
| kbitx | kbitx |  | kbitx-only | 611.5 | 1,195,317 | 5044 | 19641 | 500 | 0 |
| pbf | pbf |  | pbf-only | 611.6 | 1,605,536 | 1586 | 6212 | 457 | 0 |
| hbf | hbf |  | hbf-only | 611.7 | 9,795,926 | 2621 | 7545 | 485 | 0 |
| draw | draw |  | draw-only | 611.7 | 2,121,425 | 1104 | 4851 | 470 | 0 |
| raw | raw |  | raw-only | 611.6 | 342,217 | 1139 | 4378 | 420 | 0 |
| cpi | cpi |  | cpi-only | 611.6 | 92,308 | 1973 | 8272 | 455 | 0 |
| pf2 | pf2 |  | pf2-only | 612.0 | 3,360,612 | 1995 | 4678 | 556 | 0 |
| fontx2 | fontx2 |  | fontx2-only | 611.5 | 335,253 | 1596 | 5282 | 439 | 0 |
| hzk | hzk |  | hzk-only | 611.3 | 234,962 | 1191 | 4257 | 405 | 0 |
| nfnt | nfnt |  | nfnt-only | 611.8 | 1,056,908 | 3617 | 11738 | 519 | 0 |
| bmfont | bmfont |  | bmfont-only | 611.3 | 3,606,738 | 8196 | 26892 | 464 | 0 |
| sfd | sfd |  | sfd-only | 611.8 | 2,333,571 | 2284 | 11342 | 533 | 0 |
| kbits | kbits |  | kbits-only | 611.8 | 4,929,609 | 1383 | 4547 | 503 | 0 |
| pk | pk |  | pk-only | 611.7 | 1,587,343 | 1418 | 5416 | 441 | 0 |
| gf | gf |  | gf-only | 611.3 | 7,408,167 | 1483 | 3433 | 512 | 0 |
| plan9 | plan9 |  | plan9-only | 612.0 | 12,645,394 | 2423 | 7467 | 498 | 0 |
| amiga | amiga |  | amiga-only | 611.2 | 4,014,975 | 2106 | 5680 | 361 | 0 |
| gem | gem |  | gem-only | 613.9 | 981,557 | 1754 | 5933 | 435 | 0 |
| snf | snf |  | snf-only | 611.7 | 5,846,736 | 1688 | 6344 | 486 | 0 |
| vfont | vfont |  | vfont-only | 611.3 | 983,988 | 1271 | 4604 | 460 | 0 |
| fzx | fzx |  | fzx-only | 611.4 | 1,095,763 | 1219 | 2158 | 373 | 0 |
| figlet | figlet |  | figlet-only | 611.7 | 382,747 | 1684 | 6108 | 426 | 0 |
| hanfont | hanfont |  | hanfont-only | 611.6 | 2,818 | 827 | 835 | 325 | 0 |
| iigs | iigs |  | iigs-only | 611.4 | 503,081 | 1820 | 4256 | 572 | 0 |
| drcs | drcs |  | drcs-only | 611.6 | 2,218,771 | 1374 | 5764 | 428 | 0 |
| palm | palm |  | palm-only | 611.6 | 201,789 | 3077 | 13156 | 483 | 0 |
| gdr | gdr |  | gdr-only | 611.8 | 496,470 | 2406 | 7433 | 339 | 0 |
| dosstart | dosstart |  | dosstart-only | 611.6 | 1,697,947 | 1561 | 5730 | 447 | 0 |
| edwin | edwin |  | edwin-only | 611.7 | 4,607,359 | 1517 | 6108 | 444 | 0 |
| mkwinfont | mkwinfont |  | mkwinfont-only | 611.8 | 1,732,359 | 1585 | 6307 | 470 | 0 |
| psf2txt | psf2txt |  | psf2txt-only | 612.0 | 4,098,606 | 1882 | 7488 | 510 | 0 |
| pcr | pcr |  | pcr-only | 611.6 | 2,258,549 | 893 | 926 | 381 | 0 |
| plus3dos | plus3dos |  | plus3dos-only | 611.4 | 4,119,217 | 1171 | 1235 | 367 | 0 |
| drhalo | drhalo |  | drhalo-only | 611.5 | 132,725 | 1243 | 4661 | 415 | 0 |
| writeon | writeon |  | writeon-only | 611.5 | 7,031,656 | 1108 | 3399 | 398 | 0 |
| daisy | daisy |  | daisy-only | 611.7 | 814,247 | 1506 | 2334 | 453 | 0 |
| polyprint | polyprint |  | polyprint-only | 611.6 | 156,933 | 1317 | 1440 | 364 | 0 |
| printshop | printshop |  | printshop-only | 611.5 | 1,087,845 | 1231 | 1481 | 361 | 0 |
| tasprint | tasprint |  | tasprint-only | 611.6 | 364,360 | 1381 | 3728 | 419 | 0 |
| pxl | pxl |  | pxl-only | 611.6 | 2,029,444 | 1814 | 7011 | 536 | 0 |
| signum | signum |  | signum-only | 611.9 | 1,033,180 | 1510 | 6292 | 463 | 0 |
| rockbox_bitmap | rockbox |  | rockbox-only | 611.9 | 1,018,733 | 1901 | 7729 | 470 | 0 |
| u8m_bitmap | u8m |  | u8m-only | 611.7 | 1,830,862 | 1567 | 6587 | 513 | 0 |
| cybiko_bitmap | cybiko |  | cybiko-only | 611.7 | 2,915,702 | 1064 | 3630 | 461 | 0 |
| playdate_bitmap | playdate |  | playdate-only | 611.4 | 5,857,268 | 6027 | 21286 | 497 | 0 |
| hmzk_bitmap | hmzk |  | hmzk-only | 611.7 | 9,194,410 | 842 | 2024 | 417 | 0 |
| sbf_bitmap | sbf |  | sbf-only | 611.3 | 2,362,923 | 991 | 3706 | 473 | 0 |
| fbit | fbit |  | fbit-only | 611.7 | 271,278 | 2278 | 7492 | 453 | 0 |
| lisa | lisa |  | lisa-only | 611.3 | 2,594,018 | 1942 | 7410 | 433 | 0 |
| mgtk | mgtk |  | mgtk-only | 611.5 | 209,832 | 1010 | 2688 | 487 | 0 |
| qpf | qpf |  | qpf-only | 611.7 | 1,943,638 | 1332 | 4483 | 466 | 0 |
| qpf2 | qpf2 |  | qpf2-only | 612.0 | 395,797 | 2494 | 8935 | 452 | 0 |
| beos | beos |  | beos-only | 612.0 | 4,805,561 | 1498 | 5336 | 489 | 0 |
| oberon | oberon |  | oberon-only | 611.6 | 3,442,011 | 1214 | 3893 | 439 | 0 |
| symbos | symbos |  | symbos-only | 611.7 | 1,016,188 | 1314 | 4840 | 496 | 0 |
| wsfont | wsfont |  | wsfont-only | 611.4 | 9,315,694 | 1326 | 3675 | 480 | 0 |
| bbc | bbc |  | bbc-only | 611.5 | 6,012,098 | 916 | 2871 | 435 | 0 |
| wyse | wyse |  | wyse-only | 611.8 | 10,570,974 | 1087 | 3363 | 513 | 0 |
| grasp | grasp |  | grasp-only | 611.5 | 8,386,529 | 838 | 1592 | 567 | 0 |
| hrcg | hrcg |  | hrcg-only | 611.6 | 340,141 | 1005 | 1143 | 379 | 0 |
| xbin | xbin |  | xbin-only | 611.4 | 897,783 | 858 | 1135 | 409 | 0 |
| consoleet | consoleet |  | consoleet-only | 612.3 | 14,219,229 | 1208 | 1954 | 510 | 0 |
| zapfont | zapfont |  | zapfont-only | 611.3 | 89,084 | 1811 | 5529 | 371 | 0 |
| comfont | comfont |  | comfont-only | 611.7 | 330,406 | 1316 | 1955 | 404 | 0 |
| vtfont | vtfont |  | vtfont-only | 611.9 | 769,071 | 2050 | 8945 | 447 | 0 |
| geos | geos |  | geos-only | 611.5 | 897,536 | 2386 | 8720 | 440 | 0 |
| gpi | gpi |  | gpi-only | 611.5 | 7,721,322 | 2118 | 5414 | 471 | 0 |
| pcgeos | pcgeos |  | pcgeos-only | 611.8 | 3,538,346 | 1672 | 4880 | 508 | 0 |
| pilfont_bitmap | pilfont |  | pilfont-only | 611.5 | 6,399,377 | 3695 | 9244 | 445 | 0 |
| sfont_bitmap | sfont |  | sfont-only | 611.6 | 1,075,480 | 6106 | 15044 | 447 | 0 |
| vlw_bitmap | vlw |  | vlw-only | 611.7 | 7,330,857 | 1274 | 3261 | 437 | 0 |
| imageset_bitmap | image |  | imageset-only | 611.5 | 6,053,505 | 4733 | 14156 | 456 | 0 |
| ssfn | ssfn |  | ssfn-only | 611.4 | 1,770,767 | 3504 | 12836 | 444 | 0 |
| pfr | pfr |  | pfr-only | 611.8 | 17,241,313 | 1932 | 4384 | 471 | 0 |
| catalyst | catalyst |  | catalyst-only | 611.6 | 257,148 | 1361 | 4487 | 433 | 0 |
| chiwriter | chiwriter |  | chiwriter-only | 611.3 | 230,185 | 1604 | 8502 | 478 | 0 |
| dashen | dashen |  | dashen-only | 611.7 | 1,052,213 | 2186 | 10963 | 498 | 0 |
| hppcl | hppcl |  | hppcl-only | 611.6 | 1,132,170 | 2005 | 10997 | 487 | 0 |
| prebuilt | prebuilt |  | prebuilt-only | 611.9 | 1,077,235 | 1584 | 3611 | 464 | 0 |
| riscos | riscos |  | riscos-only | 611.6 | 3,639,882 | 2969 | 11282 | 474 | 0 |
| xerox | xerox |  | xerox-only | 611.7 | 3,766,387 | 2473 | 7310 | 458 | 0 |
| transport | raw, bdf, ssfn |  | transport-only | 611.5 | 567,901 | 7134 | 32279 | 510 | 0 |
