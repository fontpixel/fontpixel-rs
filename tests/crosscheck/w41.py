#!/usr/bin/env python3
"""W4.1 bidirectional monobit, catalogue BDF, and external-only UPX checks.

Fixtures are independently generated. Optional FreeDOS GPL corpus stays in the
specified local output directory; no font or executable bytes are committed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import urllib.request
import zipfile
import monobit

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / 'reference/fontpixel/pipeline'))
from opf.parsers.bdf import parse_bdf

CPIDOS_URL = 'https://www.ibiblio.org/pub/micro/pc-stuff/freedos/files/repositories/1.4/base/cpidos.zip'
CPIDOS_SHA = '02baf45c4173958be283602085ec560e2a7bebe31976b8dde9adebce06309c38'


def execute_mode_cpx(packed, expected, consumer=None):
    """Independent execution oracle: FreeDOS MODE's published call convention.

    The external Unicorn engine executes the original fontpixel stub. No MODE
    or UPX executable code is copied into this test or production artifacts.
    """
    from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_MEM_WRITE
    from unicorn.x86_const import UC_X86_REG_CS, UC_X86_REG_DS, UC_X86_REG_ES, UC_X86_REG_SS, UC_X86_REG_SP, UC_X86_REG_IP, UC_X86_REG_AX, UC_X86_REG_BP, UC_X86_REG_BX, UC_X86_REG_CX, UC_X86_REG_DX, UC_X86_REG_SI, UC_X86_REG_DI, UC_X86_REG_EFLAGS
    data = bytearray(packed)
    assert data[:2] == b'\x81\xfc' and data[4:8] == b'\x77\x02\xcd\x20'
    at = data.find(b'UPX!')
    assert 0 <= at < 96 and data[at+4] >= 10
    assert data[at+24:at+27] == b'\xffFO'
    if consumer is None:
        data[4] = 0xeb
        data[at+24] = 0xcb
    # MODE starts with a paragraph-normalized buffer, then enters the same
    # bytes as a COM at CS:0100. Its return stack lives in another segment.
    base, cs, ss, sp = 0x20000, 0x1ff0, 0x7000, 0xff00
    return_cs, return_ip = 0x9000, 0x1234
    start = base
    if consumer is not None:
        cs, return_cs, return_ip = 0x1000, 0x1000, 0xff00
        start = cs*16+0x100
    return_address = return_cs*16 + return_ip
    uc = Uc(UC_ARCH_X86, UC_MODE_16)
    uc.mem_map(0, 0x100000)
    uc.mem_write(base, bytes(data))
    guard = b'\xa5'*64
    uc.mem_write(base-64,guard)
    uc.mem_write(base+58880+320,guard)
    if consumer is None:
        uc.mem_write(ss*16+sp, struct.pack('<HH', return_ip, return_cs))
    else:
        uc.mem_write(start,consumer)
        uc.mem_write(ss*16+sp,struct.pack('<HHH',return_ip,0,base//16))
    for reg, value in [(UC_X86_REG_CS,cs),(UC_X86_REG_DS,cs),(UC_X86_REG_ES,cs),(UC_X86_REG_SS,ss),(UC_X86_REG_SP,sp)]:
        uc.reg_write(reg,value)
    saved = [(UC_X86_REG_BP,0x1234),(UC_X86_REG_BX,0x2345),(UC_X86_REG_CX,0x3456),(UC_X86_REG_DX,0x4567),(UC_X86_REG_SI,0x5678),(UC_X86_REG_DI,0x6789),(UC_X86_REG_EFLAGS,0x602)]
    for reg,value in saved:
        uc.reg_write(reg,value)
    writes = [0]
    def bounded_write(uc, access, address, size, value, opaque):
        assert (base <= address and address+size <= base+58880+320) or (ss*16+sp-64 <= address and address+size <= ss*16+sp+(6 if consumer else 0)), (hex(address),size)
        writes[0] += size
    uc.hook_add(UC_HOOK_MEM_WRITE,bounded_write)
    uc.emu_start(start, return_address, timeout=10_000_000, count=5_000_000)
    assert uc.reg_read(UC_X86_REG_CS) == return_cs and uc.reg_read(UC_X86_REG_IP) == return_ip
    assert uc.reg_read(UC_X86_REG_SS) == ss and uc.reg_read(UC_X86_REG_SP) == sp+(2 if consumer else 4)
    decoded = bytearray(uc.mem_read(base,len(expected)))
    if consumer is None:
        assert decoded[0] == 0xcb
        decoded[0] = 0xff  # MODE restores the CPI signature after RETF.
    else:
        assert uc.reg_read(UC_X86_REG_AX) == 1  # Actual cpx2cpi return value.
        assert uc.reg_read(UC_X86_REG_DS) == cs and uc.reg_read(UC_X86_REG_ES) == cs
        assert all(uc.reg_read(reg) == value for reg,value in saved), 'MODE did not restore caller registers/flags'
    assert uc.mem_read(base-64,64) == guard and uc.mem_read(base+58880+320,64) == guard
    assert bytes(decoded) == expected, '8086 runtime payload differs'
    return writes[0]


def cpx_writer_checks(out, run, upx, consumer=None):
    import random
    random_source = random.Random(0x435058)
    cases = []
    for name, rows in [
        ('positions', bytes((i*37+i//16*11)&255 for i in range(4096))),
        ('long-runs', bytes(4096)),
        ('short-matches', (b'AABBBCCCCDDEEEFFFF'+bytes(range(31)))*86),
        ('header-references', (b'\xffFONT   '+bytes(range(8)))*256),
        ('dictionary', bytes(random_source.randrange(256) for _ in range(256))*16),
    ]:
        rows = rows[:4096]
        count = len(rows)//16
        cp = bytearray(cp_wire(437,16,count))
        cp[40:] = rows[:count*16]
        cases.append((name,bytes(cp),'cp'))
    long_run = bytearray(cp_wire(437,225,256))
    long_run[40:] = bytes(225*256)
    cases.append(('long-run-58k',bytes(long_run),'cp'))
    # Real independent multi-page/size CPX imports exercise normal large output.
    for path in [ROOT.parent/'local-work/w41/crosscheck-final3/EGA.CPX', ROOT.parent/'local-work/w41/crosscheck-final3/EGA18.CPX']:
        if path.is_file():
            cases.append((path.stem.lower(),path.read_bytes(),'cpx'))
    checks=[]
    for name, data, fmt in cases:
        source = out/f'{name}-source.{fmt}'
        source.write_bytes(data)
        expected = out/f'{name}-expected.cpi'
        produced = out/f'{name}-native.cpx'
        unpacked = out/f'{name}-upx.cpi'
        run('convert',fmt,'cpi',source,expected)
        run('convert',fmt,'cpx',source,produced)
        native = json.loads(run('inspect','cpx',produced).stdout)
        oracle = monobit.load(expected,format='cpi')
        assert [native_sig(f) for f in native] == [mono_sig(f) for f in oracle]
        result=subprocess.run([str(upx.resolve()),'-d',str(produced),'-o',str(unpacked)],capture_output=True)
        assert result.returncode == 0, result.stderr.decode()
        plain = expected.read_bytes()
        assert unpacked.read_bytes() == plain, 'UPX byte equality'
        written = execute_mode_cpx(produced.read_bytes(),plain)
        actual_mode = execute_mode_cpx(produced.read_bytes(),plain,consumer) if consumer else None
        checks.append({'case':name,'packed_bytes':produced.stat().st_size,'plain_bytes':len(plain),'packed_sha256':hashlib.sha256(produced.read_bytes()).hexdigest(),'plain_sha256':hashlib.sha256(plain).hexdigest(),'emulated_write_bytes':written,'mode_consumer_write_bytes':actual_mode,'checks':['native reader','independent monobit pixels','UPX exact CPI bytes','8086 execution with MODE patch/far-call/stack/buffer contract']+(['actual FreeDOS MODE cpx2cpi consumer executed'] if consumer else [])})
    return checks


def cp_wire(page, height, count=256):
    return (struct.pack('<HIH8sH6sIHHHBBBBH', 28, 0, 1, b'EGA     ', page,
                        bytes(6), 0, 1, 1, 6+count*height, height, 8, 0, 0, count)
            + bytes(((c+y)*37+11) & 255 for c in range(count) for y in range(height)))


def native_sig(font):
    return [(tuple((domain, cp) for domain, cp in g['codes']), g['width'], g['height'],
             g['x'], g['y'], g['advance'][0], bytes(g['rows'])) for g in font['glyphs']]


def mono_sig(font, unicode=False):
    return [(tuple([('unicode', ord(g.char))] if unicode else [('legacy', int.from_bytes(bytes(g.codepoint), 'big'))]),
             g.width, g.height, g.left_bearing, g.shift_up, g.advance_width, g.as_bytes())
            for g in font.glyphs]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True, help='w41_timing example executable')
    p.add_argument('--output-dir', type=Path, required=True)
    p.add_argument('--fetch-cpidos', action='store_true')
    p.add_argument('--upx', type=Path, help='external independent UPX oracle, tests only')
    p.add_argument('--catalogue-bdf', type=Path)
    p.add_argument('--c64-archive', type=Path, help='optional external all_64c.zip from the original gallery')
    p.add_argument('--cpx-writer-only', action='store_true', help='run CPX writer, UPX and Unicorn checks only')
    p.add_argument('--mode-consumer', type=Path, help='external NASM -f bin build of original MODE CPITRICK.ASM')
    p.add_argument('--nasm', type=Path, help='optional development assembler to verify checked-in original stub bytes')
    a = p.parse_args()
    out = a.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    binary = str(a.binary.resolve())
    checks = []

    def run(*args):
        r = subprocess.run([binary, *map(str, args)], capture_output=True)
        assert r.returncode == 0, (args, r.stderr.decode()[:6000])
        return r

    def inspect(fmt, path, preset=None):
        return json.loads(run('inspect', fmt, path, *([preset] if preset else [])).stdout)

    if a.cpx_writer_only:
        assert a.upx, '--upx is required for CPX writer verification'
        consumer = a.mode_consumer.read_bytes() if a.mode_consumer else None
        assembly_hash = None
        if a.nasm:
            import re
            assembled = out/'fontpixel-original-stub.bin'
            subprocess.run([str(a.nasm.resolve()),'-f','bin',str(ROOT/'crates/fontpixel/src/formats/cpi/stub.asm'),'-o',str(assembled)],check=True)
            embedded = bytes(int(v,16) for v in re.findall(r'\b0x([0-9a-f]{2})\b',(ROOT/'crates/fontpixel/src/formats/cpi/stub.rs').read_text()))
            assert assembled.read_bytes() == embedded, 'assembly differs from embedded original stub'
            assembly_hash = hashlib.sha256(embedded).hexdigest()
        checks = cpx_writer_checks(out,run,a.upx,consumer)
        import unicorn
        result={'checks':checks,'passed':len(checks),'unicorn_version':unicorn.__version__,'monobit_version':str(monobit.__version__),'mode_consumer_sha256':hashlib.sha256(consumer).hexdigest() if consumer else None,'original_stub_sha256':assembly_hash}
        (out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2))
        return

    def single_check(path, fmt, oracle, *, preset=None, unicode=False, load_options=None, save_options=None):
        load_options = load_options or {}
        save_options = save_options or {}
        native = inspect(fmt, path, preset)[0]
        other = monobit.load(path, format=oracle, **load_options)[0]
        assert native_sig(native) == mono_sig(other, unicode), (fmt, 'native reads independent source')
        rust = out / f'{path.stem}-rust.{fmt}'
        run('convert', fmt, fmt, path, rust, *([preset] if preset else []))
        assert native_sig(native) == mono_sig(monobit.load(rust, format=oracle, **load_options)[0], unicode), (fmt, 'oracle reads native')
        mb = out / f'{path.stem}-monobit.{fmt}'
        monobit.save(other, mb, format=oracle, **save_options)
        assert native_sig(inspect(fmt, mb, preset)[0]) == mono_sig(monobit.load(mb, format=oracle, **load_options)[0], unicode), (fmt, 'native reads oracle writer')
        bdf = out / f'{path.stem}.bdf'
        run('convert', fmt, 'bdf', path, bdf, *([preset] if preset else []))
        catalogue = parse_bdf(bdf, 'w41', remap_charset=False)
        expected = [(codes[0][1], w, h, x, y, advance, bitmap) for codes, w, h, x, y, advance, bitmap in native_sig(native)]
        actual = [(g.cp, g.bbw, g.bbh, g.bbx, g.bby, g.dwidth, bytes(g.rows)) for g in catalogue.glyphs]
        assert actual == expected, (fmt, 'independent catalogue BDF bridge')
        checks.append({'case': path.name, 'format': fmt, 'glyphs': len(expected), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'checks': ['native reads monobit', 'monobit reads native', 'catalogue BDF parser']})

    path = out / 'wire.draw'
    path.write_text('% generated independently\n0041:\t--##----\n\t-#--#---\n\n0042:\t######--\n\t#----#--\n\n', encoding='ascii')
    single_check(path, 'draw', 'hexdraw', unicode=True)
    for preset, height, count in [('vga8',8,256), ('vga14',14,256), ('vga16',16,256), ('ch8',8,128), ('msx',8,256), ('c64',8,256), ('spectrum',8,96)]:
        path = out / f'wire-{preset}.raw'
        path.write_bytes(bytes((i*37+11)&255 for i in range(height*count)))
        single_check(path, 'raw', 'raw', preset=preset, load_options={'cell': (8,height), 'first_codepoint': 32 if preset=='spectrum' else 0})

    for count, address in [(64, 0x2000), (256, 0x3800)]:
        path = out / f'wire-{count}.64c'
        path.write_bytes(struct.pack('<H', address) + bytes((i*37+11)&255 for i in range(8*count)))
        single_check(path, 'raw', '64c', preset='64c', load_options={'charset': ''}, save_options={'charset': ''})
        produced = out / f'{path.stem}-rust.raw'
        # Independent PRG consumer semantics: the destination comes from the LE header.
        assert struct.unpack('<H', produced.read_bytes()[:2])[0] == address
        assert produced.read_bytes()[2:] == path.read_bytes()[2:]
    if a.c64_archive:
        data = a.c64_archive.read_bytes()
        assert hashlib.sha256(data).hexdigest() == '49a7bde696a84b28614b214126ace74397bb491772289a461f3491b0336fc776'
        with zipfile.ZipFile(a.c64_archive) as z:
            for count in [64, 256]:
                name = next(n for n in sorted(z.namelist()) if n.lower().endswith('.64c') and len(z.read(n)) == 2+count*8)
                path = out / f'gallery-{count}-{Path(name).name}'
                path.write_bytes(z.read(name))
                single_check(path, 'raw', '64c', preset='64c', load_options={'charset': ''}, save_options={'charset': ''})

    source_fonts = []
    for page in [437,850]:
        for height in [8,16]:
            path = out / f'wire-{page}-{height}.cp'
            path.write_bytes(cp_wire(page,height))
            native = inspect('cpi',path)[0]
            oracle = monobit.load(path,format='kbd')[0]
            assert native_sig(native) == mono_sig(oracle)
            source_fonts.append(oracle)
            if page == 437 and height == 8:
                single_check(path, 'cp', 'kbd')
    for version, fmt in [('FONT','cpi'),('FONT.NT','fontnt'),('DRFONT','drfont')]:
        mb = out / f'monobit-{version}.cpi'
        monobit.save(source_fonts[:1] if version=='DRFONT' else source_fonts,mb,format='cpi',version=version)
        native = inspect('cpi',mb)
        oracle = monobit.load(mb,format='cpi')
        assert [native_sig(f) for f in native] == [mono_sig(f) for f in oracle], version
        rust = out / f'rust-{version}.cpi'
        run('convert','cpi',fmt,mb,rust)
        assert [native_sig(f) for f in inspect('cpi',rust)] == [mono_sig(f) for f in monobit.load(rust,format='cpi')], version
        checks.append({'case':version,'fonts':len(native),'glyphs':sum(len(f['glyphs']) for f in native),'checks':['single-page single-size monobit bidirectional' if version=='DRFONT' else 'multi-page multi-size monobit bidirectional']})

    # monobit DRFONT writer has a known multi-page pointer/table-order defect.
    # Keep the original bad output and verify our correctly shared tables independently.
    probe = out / 'monobit-DRFONT-multipage-original.cpi'
    monobit.save(source_fonts,probe,format='cpi',version='DRFONT')
    rejection = subprocess.run([binary,'inspect','cpi',str(probe)],capture_output=True)
    assert rejection.returncode != 0
    (out/'monobit-DRFONT-multipage-rejection.txt').write_bytes(rejection.stderr)
    native_dr = out / 'native-DRFONT-multipage.cpi'
    run('convert','cpi','drfont',out/'monobit-FONT.cpi',native_dr)
    expected = [native_sig(f) for f in inspect('cpi',out/'monobit-FONT.cpi')]
    assert expected == [mono_sig(f) for f in monobit.load(native_dr,format='cpi')]
    checks.append({'case':'DRFONT multi-page multi-size','fonts':4,'glyphs':1024,'checks':['native writer to independent monobit reader'],'oracle_limitation':'monobit writer next pointer misses 28-byte entry and writes bitmap tables in page order','original_sha256':hashlib.sha256(probe.read_bytes()).hexdigest()})

    if a.catalogue_bdf:
        draw = out / 'catalogue.draw'
        run('convert','bdf','draw',a.catalogue_bdf,draw)
        parsed = parse_bdf(a.catalogue_bdf,'w41',remap_charset=False)
        expected = [(g.cp,g.bbw,g.bbh,bytes(g.rows)) for g in parsed.glyphs if g.cp is not None and 0<=g.cp<=0x10ffff and not 0xd800<=g.cp<=0xdfff]
        native = inspect('draw',draw)[0]
        actual = [(g['codes'][0][1],g['width'],g['height'],bytes(g['rows'])) for g in native['glyphs']]
        assert actual == expected, 'catalogue source pixels through hexdraw'
        checks.append({'case':str(a.catalogue_bdf),'glyphs':len(actual),'checks':['actual catalogue raster equality']})

    if a.fetch_cpidos:
        data = urllib.request.urlopen(CPIDOS_URL,timeout=30).read(1024*1024)
        assert hashlib.sha256(data).hexdigest()==CPIDOS_SHA
        archive = out / 'cpidos.zip'
        archive.write_bytes(data)
        with zipfile.ZipFile(archive) as z:
            (out/'CPIDOS-COPYING.TXT').write_bytes(z.read('DOC/CPIDOS/COPYING.TXT'))
            for name in sorted(n for n in z.namelist() if n.lower().endswith('.cpx')):
                path=out/Path(name).name
                path.write_bytes(z.read(name))
                native=inspect('cpx',path)
                rust=out/f'{path.stem}-native.cpi'
                run('convert','cpx','cpi',path,rust)
                assert [native_sig(f) for f in native]==[mono_sig(f) for f in monobit.load(rust,format='cpi')]
                if a.upx:
                    unpacked=out/f'{path.stem}-upx.cpi'
                    r=subprocess.run([str(a.upx.resolve()),'-d',str(path),'-o',str(unpacked)],capture_output=True)
                    assert r.returncode==0,r.stderr.decode()
                    assert [native_sig(f) for f in native]==[mono_sig(f) for f in monobit.load(unpacked,format='cpi')], name
                checks.append({'case':name,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'fonts':len(native),'glyphs':sum(len(f['glyphs']) for f in native),'checks':['native CPX to monobit CPI']+(['external UPX pixel/metric equality'] if a.upx else [])})
    result={'checks':checks,'passed':len(checks),'monobit_version':str(monobit.__version__),'cpidos_url':CPIDOS_URL if a.fetch_cpidos else None,'cpidos_license':'GPL-2.0, downloaded test oracle only' if a.fetch_cpidos else None}
    (out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__=='__main__':
    main()
