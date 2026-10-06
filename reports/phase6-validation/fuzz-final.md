# Phase 6 final ASAN fuzz campaign (strict source)

Label `phase6-strict-v5-20261005T225900Z` (gate 077) on the final strict source `8cd6d66`, evaluated 2026-10-06T00:50:51.241808+00:00. Criterion: exit==0 and no crash-/timeout-/oom-/leak- artifacts and elapsed>=required and libFuzzer final stats present with executions>0

Limits: libFuzzer `-max_total_time=610`, `-rss_limit_mb=2048`, `-timeout=10`, 12 targets in parallel; AddressSanitizer with leak detection (defaults).

Result: 104/104 targets passed, 0 failed, 298,741,042 executions in total. Every one of the 99 reader IDs is reached by at least one target. Earlier campaigns on superseded sources (2026-10-05: 301,696,538 and 261,993,213 executions) also passed 104/104.

| Target | Reader entry points | Also reaches | Profile | Elapsed (s) | Executions | cov | ft | Peak RSS (MB) | Exit |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| bdf | bdf |  | baseline | 612.9 | 259,140 | 3739 | 16190 | 567 | 0 |
| pcf | pcf |  | baseline | 611.8 | 938,281 | 4875 | 15564 | 488 | 0 |
| yaff | yaff |  | baseline | 611.8 | 950,360 | 5833 | 23559 | 540 | 0 |
| sfnt |  | ttf, otf | baseline | 612.7 | 2,168,276 | 3052 | 8628 | 498 | 0 |
| glyf |  | ttf | baseline | 612.3 | 1,868,665 | 1636 | 6882 | 498 | 0 |
| compat_ttf |  | ttf | baseline | 611.7 | 44,001 | 3756 | 13307 | 767 | 0 |
| cff |  | otf | baseline | 611.9 | 4,539,703 | 2628 | 7657 | 454 | 0 |
| embedded_bitmap | otb, bdat |  | embedded-only | 612.1 | 2,888,927 | 5300 | 12630 | 490 | 0 |
| color_bitmap | color |  | color-only | 611.3 | 188,639 | 6656 | 24379 | 485 | 0 |
| woff1 | woff |  | baseline | 611.2 | 5,970,056 | 1118 | 3400 | 398 | 0 |
| woff2 | woff2 |  | baseline | 611.5 | 1,426,321 | 3796 | 12987 | 465 | 0 |
| gsub |  | ttf, otf | baseline | 611.5 | 21,113,700 | 1688 | 4642 | 523 | 0 |
| subset |  | ttf, otf | baseline | 612.0 | 3,036,399 | 6045 | 16571 | 466 | 0 |
| vector_raster | vector-raster, ttf, otf, woff, woff2, ttc |  | baseline | 612.6 | 354,047 | 10676 | 32919 | 586 | 0 |
| collection | ttc | woff, woff2 | baseline | 611.4 | 3,350,827 | 1353 | 2878 | 427 | 0 |
| svg | svg |  | baseline | 611.7 | 983,116 | 4694 | 17489 | 493 | 0 |
| hex | hex |  | baseline | 612.0 | 3,345,933 | 1049 | 5394 | 522 | 0 |
| psf | psf |  | baseline | 611.5 | 777,795 | 1229 | 4897 | 420 | 0 |
| fnt | fnt |  | baseline | 611.3 | 384,332 | 1667 | 4984 | 422 | 0 |
| fon | fon | fnt | fon-only | 613.2 | 10,625,396 | 688 | 1279 | 559 | 0 |
| image | image |  | image-c | 611.4 | 1,747,531 | 12070 | 38975 | 693 | 0 |
| c | c |  | c-only | 611.2 | 421,187 | 7432 | 28342 | 560 | 0 |
| kbitx | kbitx |  | kbitx-only | 611.3 | 1,238,202 | 5039 | 19418 | 492 | 0 |
| pbf | pbf |  | pbf-only | 611.6 | 1,682,392 | 1584 | 6200 | 454 | 0 |
| hbf | hbf |  | hbf-only | 611.8 | 7,340,894 | 2627 | 7530 | 459 | 0 |
| draw | draw |  | draw-only | 611.3 | 1,835,734 | 1104 | 4841 | 487 | 0 |
| raw | raw |  | raw-only | 611.6 | 331,084 | 1140 | 4379 | 417 | 0 |
| cpi | cpi |  | cpi-only | 612.0 | 109,804 | 1983 | 7727 | 438 | 0 |
| pf2 | pf2 |  | pf2-only | 612.8 | 4,996,223 | 2099 | 4746 | 557 | 0 |
| fontx2 | fontx2 |  | fontx2-only | 611.3 | 427,925 | 1598 | 5295 | 443 | 0 |
| hzk | hzk |  | hzk-only | 611.9 | 245,534 | 1193 | 4277 | 408 | 0 |
| nfnt | nfnt |  | nfnt-only | 611.5 | 1,283,070 | 3617 | 11667 | 535 | 0 |
| bmfont | bmfont |  | bmfont-only | 611.2 | 3,089,349 | 8106 | 26755 | 464 | 0 |
| sfd | sfd |  | sfd-only | 611.5 | 2,053,534 | 2281 | 11446 | 507 | 0 |
| kbits | kbits |  | kbits-only | 612.0 | 3,831,436 | 1382 | 4607 | 518 | 0 |
| pk | pk |  | pk-only | 612.1 | 2,238,990 | 1529 | 5454 | 498 | 0 |
| gf | gf |  | gf-only | 611.9 | 6,684,508 | 1636 | 3615 | 488 | 0 |
| plan9 | plan9 |  | plan9-only | 611.9 | 11,765,310 | 2486 | 7636 | 494 | 0 |
| amiga | amiga |  | amiga-only | 611.8 | 4,301,676 | 2103 | 5766 | 487 | 0 |
| gem | gem |  | gem-only | 611.8 | 1,111,693 | 1634 | 5323 | 416 | 0 |
| snf | snf |  | snf-only | 612.0 | 4,223,337 | 1697 | 6357 | 449 | 0 |
| vfont | vfont |  | vfont-only | 611.9 | 762,504 | 1274 | 4623 | 451 | 0 |
| fzx | fzx |  | fzx-only | 612.1 | 1,033,168 | 1222 | 2164 | 372 | 0 |
| figlet | figlet |  | figlet-only | 611.3 | 331,426 | 1680 | 6113 | 423 | 0 |
| hanfont | hanfont |  | hanfont-only | 612.9 | 2,968 | 827 | 835 | 324 | 0 |
| iigs | iigs |  | iigs-only | 611.5 | 598,906 | 1821 | 4379 | 592 | 0 |
| drcs | drcs |  | drcs-only | 611.3 | 2,102,446 | 1374 | 5939 | 421 | 0 |
| palm | palm |  | palm-only | 611.7 | 178,368 | 3078 | 12944 | 484 | 0 |
| gdr | gdr |  | gdr-only | 611.6 | 398,520 | 2416 | 7594 | 347 | 0 |
| dosstart | dosstart |  | dosstart-only | 611.8 | 1,628,700 | 1567 | 5770 | 443 | 0 |
| edwin | edwin |  | edwin-only | 611.6 | 4,001,537 | 1522 | 6113 | 468 | 0 |
| mkwinfont | mkwinfont |  | mkwinfont-only | 611.8 | 1,722,007 | 1595 | 6319 | 475 | 0 |
| psf2txt | psf2txt |  | psf2txt-only | 612.2 | 3,142,805 | 1918 | 7465 | 495 | 0 |
| pcr | pcr |  | pcr-only | 611.8 | 1,775,654 | 896 | 929 | 380 | 0 |
| plus3dos | plus3dos |  | plus3dos-only | 611.8 | 3,658,712 | 1176 | 1240 | 367 | 0 |
| drhalo | drhalo |  | drhalo-only | 611.2 | 133,036 | 1248 | 4679 | 410 | 0 |
| writeon | writeon |  | writeon-only | 612.2 | 5,742,271 | 1113 | 3424 | 501 | 0 |
| daisy | daisy |  | daisy-only | 612.4 | 765,542 | 1537 | 2787 | 442 | 0 |
| polyprint | polyprint |  | polyprint-only | 611.8 | 148,785 | 1323 | 1447 | 364 | 0 |
| printshop | printshop |  | printshop-only | 611.7 | 982,040 | 1233 | 1485 | 360 | 0 |
| tasprint | tasprint |  | tasprint-only | 611.8 | 319,530 | 1385 | 3747 | 414 | 0 |
| pxl | pxl |  | pxl-only | 612.4 | 983,296 | 1822 | 7080 | 443 | 0 |
| signum | signum |  | signum-only | 611.8 | 790,770 | 1513 | 6307 | 435 | 0 |
| rockbox_bitmap | rockbox |  | rockbox-only | 611.7 | 1,067,978 | 1914 | 7818 | 561 | 0 |
| u8m_bitmap | u8m |  | u8m-only | 611.4 | 1,604,856 | 1573 | 6661 | 507 | 0 |
| cybiko_bitmap | cybiko |  | cybiko-only | 611.9 | 1,719,539 | 1066 | 3607 | 411 | 0 |
| playdate_bitmap | playdate |  | playdate-only | 611.2 | 6,106,747 | 6051 | 21536 | 503 | 0 |
| hmzk_bitmap | hmzk |  | hmzk-only | 611.9 | 7,936,140 | 845 | 2032 | 417 | 0 |
| sbf_bitmap | sbf |  | sbf-only | 611.8 | 1,593,433 | 993 | 3919 | 447 | 0 |
| fbit | fbit |  | fbit-only | 611.4 | 245,216 | 2279 | 7485 | 451 | 0 |
| lisa | lisa |  | lisa-only | 611.8 | 4,127,856 | 1959 | 7505 | 461 | 0 |
| mgtk | mgtk |  | mgtk-only | 612.2 | 165,708 | 1011 | 2662 | 425 | 0 |
| qpf | qpf |  | qpf-only | 612.2 | 1,091,486 | 1332 | 4604 | 464 | 0 |
| qpf2 | qpf2 |  | qpf2-only | 612.5 | 389,976 | 2511 | 9001 | 452 | 0 |
| beos | beos |  | beos-only | 612.3 | 3,965,777 | 1501 | 5331 | 488 | 0 |
| oberon | oberon |  | oberon-only | 611.8 | 4,324,865 | 1215 | 3824 | 441 | 0 |
| symbos | symbos |  | symbos-only | 611.8 | 880,153 | 1317 | 4834 | 448 | 0 |
| wsfont | wsfont |  | wsfont-only | 612.0 | 8,020,458 | 1312 | 3264 | 474 | 0 |
| bbc | bbc |  | bbc-only | 612.0 | 4,916,420 | 918 | 2948 | 511 | 0 |
| wyse | wyse |  | wyse-only | 611.9 | 13,416,065 | 1086 | 3295 | 451 | 0 |
| grasp | grasp |  | grasp-only | 612.2 | 8,215,402 | 839 | 1661 | 519 | 0 |
| hrcg | hrcg |  | hrcg-only | 612.0 | 326,616 | 1007 | 1145 | 378 | 0 |
| xbin | xbin |  | xbin-only | 611.8 | 990,709 | 859 | 1114 | 402 | 0 |
| consoleet | consoleet |  | consoleet-only | 611.5 | 12,788,974 | 1211 | 1962 | 514 | 0 |
| zapfont | zapfont |  | zapfont-only | 611.9 | 94,176 | 1878 | 5686 | 366 | 0 |
| comfont | comfont |  | comfont-only | 612.0 | 280,264 | 1317 | 1959 | 429 | 0 |
| vtfont | vtfont |  | vtfont-only | 611.8 | 882,367 | 2053 | 8945 | 450 | 0 |
| geos | geos |  | geos-only | 611.6 | 662,280 | 2362 | 8031 | 436 | 0 |
| gpi | gpi |  | gpi-only | 611.8 | 8,803,783 | 2118 | 5422 | 508 | 0 |
| pcgeos | pcgeos |  | pcgeos-only | 612.2 | 2,534,191 | 1680 | 4905 | 489 | 0 |
| pilfont_bitmap | pilfont |  | pilfont-only | 611.7 | 5,377,063 | 3698 | 9476 | 447 | 0 |
| sfont_bitmap | sfont |  | sfont-only | 611.3 | 1,181,478 | 6230 | 15579 | 432 | 0 |
| vlw_bitmap | vlw |  | vlw-only | 611.9 | 5,756,145 | 1280 | 3248 | 456 | 0 |
| imageset_bitmap | image |  | imageset-only | 611.4 | 6,332,346 | 4700 | 13826 | 458 | 0 |
| ssfn | ssfn |  | ssfn-only | 611.3 | 2,298,842 | 3530 | 12990 | 461 | 0 |
| pfr | pfr |  | pfr-only | 612.1 | 11,747,176 | 1934 | 4283 | 468 | 0 |
| catalyst | catalyst |  | catalyst-only | 611.6 | 233,229 | 1362 | 4537 | 433 | 0 |
| chiwriter | chiwriter |  | chiwriter-only | 612.2 | 231,116 | 1605 | 8528 | 455 | 0 |
| dashen | dashen |  | dashen-only | 611.5 | 1,169,441 | 2194 | 11023 | 502 | 0 |
| hppcl | hppcl |  | hppcl-only | 612.0 | 1,111,892 | 2011 | 10938 | 492 | 0 |
| prebuilt | prebuilt |  | prebuilt-only | 612.1 | 800,126 | 1583 | 3618 | 459 | 0 |
| riscos | riscos |  | riscos-only | 611.6 | 4,882,968 | 2966 | 11189 | 476 | 0 |
| xerox | xerox |  | xerox-only | 612.2 | 2,467,642 | 2470 | 7297 | 468 | 0 |
| transport | raw, bdf, ssfn |  | transport-only | 611.2 | 623,825 | 7128 | 32230 | 511 | 0 |
