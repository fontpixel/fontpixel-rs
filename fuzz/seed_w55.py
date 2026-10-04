#!/usr/bin/env python3
"""Generate original W5.5 native C/DRCS corpus using the verified bridge."""
import argparse,pathlib,subprocess
p=argparse.ArgumentParser();p.add_argument('--bridge',type=pathlib.Path,required=True);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args()
for name in ['c','drcs']:(a.out/name).mkdir(parents=True,exist_ok=True)
for family,selector in [('u8g2',5),('lvgl',6),('lvgl-rle',6),('lvgl-xor',6),('lvgl4-xor',6),('lvgl8',6),('lvgl4-binary-xor',10),('netbsd',7),('u8glib',8),('u8glib-compact',8),('u8glib-gray',8),('u8x8',9),('u8x8-legacy',20),('drcs',0)]:
    dest=a.out/('drcs' if family=='drcs' else 'c')/family
    subprocess.run([str(a.bridge),'fixture',family,str(dest)],check=True)
    raw=dest.read_bytes()
    if family!='drcs':dest.write_bytes(bytes([selector])+raw)
    else:
        (a.out/'drcs'/'c1').write_bytes(raw.replace(b'\x1bP',b'\x90').replace(b'\x1b\\',b'\x9c'))
        (a.out/'drcs'/'partial').write_bytes(raw.replace(b';2;8;',b';1;8;',1))
for i,raw in enumerate([b'\x1bP{ @?\x1b\\',b'\x1bP0;0;0;2;0;0;0;0{ @~/?????\x1b\\',b'\x90;;;;;;;;{ @\x9c',b'\x1bP0;0;2;5;0;2;1;1{ @?????;\x1b\\']):(a.out/'drcs'/f'control-{i}').write_bytes(raw)
print(a.out)

raw=(a.out/'c'/'lvgl').read_bytes()
(a.out/'c'/'lvgl-guards').write_bytes(raw[:1]+b'#if LV_VERSION_CHECK(8,0,0)\n'+raw[1:]+b'\n#else\ninvalid unused branch\n#endif\n')
