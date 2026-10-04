//! Original instruction emitters. Only native recognition signatures are inherited data.
use super::*;
fn word(v: &mut Vec<u8>, value: u16) {
    v.extend_from_slice(&value.to_le_bytes());
}
fn instruction(v: &mut Vec<u8>, op: u8, value: u16) {
    v.push(op);
    word(v, value);
}
fn bios(font: u16, height: u8) -> Vec<u8> {
    let mut v = vec![0x0e, 0x07, 0xb8, 0x10, 0x11, 0x31, 0xdb, 0xb7, height];
    instruction(&mut v, 0xbd, font);
    instruction(&mut v, 0xb9, 256);
    v.extend_from_slice(&[0x31, 0xd2, 0xcd, 0x10]);
    v
}
fn patch16(s: &mut [u8], at: usize, value: usize) -> Result<()> {
    let value = u16::try_from(value).map_err(|_| invalid("COM address overflow"))?;
    s[at..at + 2].copy_from_slice(&value.to_le_bytes());
    Ok(())
}
fn near(s: &mut [u8], at: usize, target: usize) {
    let displacement = (target as isize - at as isize - 2) as i16;
    s[at..at + 2].copy_from_slice(&displacement.to_le_bytes());
}
fn tsr(out: &mut Vec<u8>, height: u8) -> Result<()> {
    let entry = out.len();
    let mut code = Vec::new();
    instruction(&mut code, 0xb8, 0x3510);
    code.extend_from_slice(&[
        0xcd, 0x21, 0x2e, 0x89, 0x1e, 3, 1, 0x2e, 0x8c, 0x06, 5, 1, 0x0e, 0x1f,
    ]);
    let handler_operand = code.len() + 1;
    instruction(&mut code, 0xba, 0);
    instruction(&mut code, 0xb8, 0x2510);
    code.extend_from_slice(&[0xcd, 0x21]);
    let init_call = code.len() + 1;
    instruction(&mut code, 0xe8, 0);
    let paragraphs = code.len() + 1;
    instruction(&mut code, 0xba, 0);
    instruction(&mut code, 0xb8, 0x3100);
    code.extend_from_slice(&[0xcd, 0x21]);
    let handler = code.len();
    code.extend_from_slice(&[0x80, 0xfc, 0, 0x75, 0, 0x3c, 3, 0x76, 4, 0x3c, 7, 0x75, 0]);
    let branch1 = handler + 4;
    let branch2 = handler + 12;
    code.extend_from_slice(&[0x9c, 0x2e, 0xff, 0x1e, 3, 1, 0x9c, 0x60, 0x1e, 0x06]);
    let handler_call = code.len() + 1;
    instruction(&mut code, 0xe8, 0);
    code.extend_from_slice(&[0x07, 0x1f, 0x61, 0x9d, 0xcf]);
    let chain = code.len();
    code.extend_from_slice(&[0x2e, 0xff, 0x2e, 3, 1]);
    let install = code.len();
    code.extend_from_slice(&bios(0x163, height));
    code.push(0xc3);
    code[branch1] = (chain - branch1 - 1) as u8;
    code[branch2] = (chain - branch2 - 1) as u8;
    near(&mut code, init_call, install);
    near(&mut code, handler_call, install);
    patch16(&mut code, handler_operand, 0x100 + entry + handler)?;
    let resident = (0x100 + entry + code.len()).div_ceil(16);
    patch16(&mut code, paragraphs, resident)?;
    out[0] = 0xe9;
    patch16(out, 1, entry - 3)?;
    out.extend_from_slice(&code);
    Ok(())
}
fn cpm_destination(target: CpmTarget) -> u16 {
    match target {
        CpmTarget::Pcw => 0xb800,
        CpmTarget::Cpc => 0x8000,
        CpmTarget::Spectrum => 0x3000,
    }
}
fn relocated_address(file: usize, origin: usize, destination: usize) -> Result<u16> {
    u16::try_from(file - origin + destination).map_err(|_| invalid("CP/M relocation exceeds 64K"))
}
fn cpm_legacy(out: &mut [u8], variant: ComfontVariant, target: CpmTarget) -> Result<()> {
    let (dest, copied, font, copy_routine) = if variant == ComfontVariant::Letafont {
        (0xe290usize, 0x864usize, 44usize, 2092usize)
    } else {
        (0xe22c, 0x834, 114, 64)
    };
    let prefix = if variant == ComfontVariant::Letafont {
        LETAFONT_MAGIC
    } else {
        UDG_MAGIC
    };
    out[..prefix.len()].copy_from_slice(prefix);
    let base = 14;
    let mut main = vec![0x2a, 1, 0, 0x11, 0x57, 0, 0x19, 0x22, 0, 0];
    let patch_operand = base + 8;
    // UDG's historical relocation covers 250 slots; explicitly finish the last six.
    if variant == ComfontVariant::Udg {
        instruction(&mut main, 0x21, (0x100 + base + copied) as u16);
        instruction(&mut main, 0x11, (dest + copied) as u16);
        instruction(&mut main, 0x01, (font + 2048 - base - copied) as u16);
        main.extend_from_slice(&[0xed, 0xb0]);
    }
    let routine = relocated_address(copy_routine, base, dest)?;
    instruction(&mut main, 0x01, routine);
    let call_operand = base + main.len() + 1;
    instruction(&mut main, 0xcd, 0);
    word(
        &mut main,
        if target == CpmTarget::Pcw {
            0xe9
        } else {
            routine
        },
    );
    instruction(&mut main, 0xc3, 0);
    if base + main.len() > font {
        return Err(invalid("CP/M header program overlaps font"));
    }
    out[base..base + main.len()].copy_from_slice(&main);
    patch16(
        out,
        patch_operand,
        usize::from(relocated_address(call_operand, base, dest)?),
    )?;
    let mut copy = Vec::new();
    instruction(&mut copy, 0x21, relocated_address(font, base, dest)?);
    instruction(&mut copy, 0x11, cpm_destination(target));
    instruction(&mut copy, 0x01, 2048);
    copy.extend_from_slice(&[0xed, 0xb0, 0xc9]);
    out[copy_routine..copy_routine + copy.len()].copy_from_slice(&copy);
    Ok(())
}
fn psfcom(out: &mut [u8], height: u8, count: usize, target: CpmTarget) -> Result<()> {
    out[..6].copy_from_slice(&[0xeb, 4, 0xeb, 0xc3, 0x40, 1]);
    out[6..11].copy_from_slice(&[0xb8, 1, 0x4c, 0xcd, 0x21]);
    let sig = if height == 16 { PSFCOM16 } else { PSFCOM8 };
    out[15..46].copy_from_slice(sig);
    patch16(out, 46, count * usize::from(height))?;
    patch16(out, 48, 0x300)?;
    let count = if target == CpmTarget::Spectrum {
        count
    } else {
        count.min(256)
    };
    let mut code = vec![0x2a, 1, 0, 0x11, 0x57, 0, 0x19, 0x22, 0, 0];
    let call_patch = 8;
    let routine_pointer = code.len() + 1;
    instruction(&mut code, 0x01, 0);
    let call_operand = code.len() + 1;
    instruction(&mut code, 0xcd, 0);
    let function = code.len();
    word(&mut code, 0);
    instruction(&mut code, 0xc3, 0);
    let copy = code.len();
    instruction(&mut code, 0x21, 0xc400);
    instruction(&mut code, 0x11, cpm_destination(target));
    instruction(&mut code, 0x01, (count * 8) as u16);
    if height == 8 {
        code.extend_from_slice(&[0xed, 0xb0]);
    } else {
        code.extend_from_slice(&[0x7e, 0x12, 0x23, 0x23, 0x13, 0x0b, 0x78, 0xb1, 0x20, 0xf6]);
    }
    code.push(0xc9);
    patch16(&mut code, call_patch, 0xe800 + call_operand)?;
    patch16(&mut code, routine_pointer, 0xe800 + copy)?;
    patch16(
        &mut code,
        function,
        if target == CpmTarget::Pcw {
            0xe9
        } else {
            0xe800 + copy
        },
    )?;
    let mut entry = Vec::new();
    instruction(&mut entry, 0x21, 0x300);
    instruction(&mut entry, 0x11, 0xc400);
    instruction(&mut entry, 0x01, (count * usize::from(height)) as u16);
    entry.extend_from_slice(&[0xed, 0xb0]);
    instruction(&mut entry, 0x21, 0x180);
    instruction(&mut entry, 0x11, 0xe800);
    instruction(&mut entry, 0x01, code.len() as u16);
    entry.extend_from_slice(&[0xed, 0xb0]);
    instruction(&mut entry, 0xc3, 0xe800);
    out[64..64 + entry.len()].copy_from_slice(&entry);
    out[128..128 + code.len()].copy_from_slice(&code);
    out[256..281].copy_from_slice(b"fontpixel original loader");
    Ok(())
}
pub(super) fn wrap(
    rows: &[u8],
    height: u8,
    variant: ComfontVariant,
    target: CpmTarget,
    b: &mut Budget,
) -> Result<Vec<u8>> {
    let count = rows.len() / usize::from(height);
    let offset = match variant {
        ComfontVariant::Frapt => 25,
        ComfontVariant::Fontedit | ComfontVariant::FraptTsr => 99,
        ComfontVariant::Mania => 128,
        ComfontVariant::Letafont => 44,
        ComfontVariant::Udg => 114,
        ComfontVariant::Psfcom => 512,
    };
    let total = (offset + rows.len()).max(if variant == ComfontVariant::Letafont {
        14 + 0x864
    } else {
        0
    });
    let mut out = allocated(
        total
            + if variant == ComfontVariant::FraptTsr {
                256
            } else {
                0
            },
        b,
    )?;
    out.truncate(total);
    out[offset..offset + rows.len()].copy_from_slice(rows);
    match variant {
        ComfontVariant::Frapt => {
            out[..25].copy_from_slice(&[
                0x56, 0x49, 0x4c, 0x45, 0x1a, 0xc0, 0x44, 0x5e, 0xb8, 0x10, 0x11, 0xbd, 0x19, 1,
                0xb9, 0, 1, 0x99, 0x90, 0xbb, 0, height, 0xcd, 0x10, 0xc3,
            ]);
        }
        ComfontVariant::FraptTsr => {
            out[40..45].copy_from_slice(b"VILE\x1a");
            out[93] = height;
            tsr(&mut out, height)?;
        }
        ComfontVariant::Fontedit => {
            out[..FONTEDIT_MAGIC.len()].copy_from_slice(FONTEDIT_MAGIC);
            out[50] = height;
            let mut code = bios(0x163, height);
            code.extend_from_slice(&[0xb8, 0, 0x4c, 0xcd, 0x21]);
            out[53..53 + code.len()].copy_from_slice(&code);
        }
        ComfontVariant::Mania => {
            out[0] = 0xeb;
            out[1] = 78;
            patch16(&mut out, 2, 128)?;
            patch16(&mut out, 4, rows.len())?;
            out[6..8].copy_from_slice(b"\0 ");
            let message = b"FONT MANIA, VERSION compatible; fontpixel original loader";
            out[8..8 + message.len()].copy_from_slice(message);
            let mut code = bios(0x180, height);
            code.extend_from_slice(&[0xb8, 0, 0x4c, 0xcd, 0x21]);
            out[80..80 + code.len()].copy_from_slice(&code);
        }
        ComfontVariant::Letafont | ComfontVariant::Udg => cpm_legacy(&mut out, variant, target)?,
        ComfontVariant::Psfcom => psfcom(&mut out, height, count, target)?,
    }
    Ok(out)
}
