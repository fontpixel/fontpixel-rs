//! Original bounded NRV2B encoder and CPX executable packaging.
//! The wire grammar follows the MIT-referenced reader. The assembly loader
//! is original project code; no UPX/UCL implementation or stub is included.
use super::*;
#[path = "stub.rs"]
mod stub;

const MODE_INPUT: usize = 58880;
const MODE_EXTRA: usize = 320;
const HEADER: usize = 22;
const ENTRY: usize = stub::BOOTSTRAP.len() + HEADER;
const EMPTY: u16 = u16::MAX;

struct Stream {
    bytes: Vec<u8>,
    word: usize,
    used: u8,
}
impl Stream {
    fn bit(&mut self, one: bool) {
        if self.used == 0 {
            self.word = self.bytes.len();
            self.bytes.extend_from_slice(&[0; 2]);
        }
        if one {
            let bit = 15 - self.used;
            self.bytes[self.word + usize::from(bit / 8)] |= 1 << (bit % 8);
        }
        self.used = (self.used + 1) % 16;
    }
    fn number(&mut self, n: u32) {
        for shift in (0..31 - n.leading_zeros()).rev() {
            self.bit(n & (1 << shift) != 0);
            self.bit(shift == 0);
        }
    }
}
fn hash(b: &[u8], pos: usize) -> usize {
    (usize::from(b[pos]) * 251 + usize::from(b[pos + 1]) * 31 + usize::from(b[pos + 2])) & 4095
}
fn put_word(b: &mut [u8], at: usize, n: usize) -> Result<()> {
    let n = u16::try_from(n).map_err(|_| invalid("CPX 16-bit address overflow"))?;
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
    Ok(())
}

pub(super) fn pack(
    cpi: Vec<u8>,
    o: &CpiWriteOptions,
    budget: &mut BoundedReport,
) -> Result<Vec<u8>> {
    let n = cpi.len();
    if n > MODE_INPUT {
        return Err(invalid(
            "CPX decoded FONT exceeds FreeDOS MODE's 58880-byte limit",
        ));
    }
    if !cpi.starts_with(b"\xffFONT   ") {
        return Err(invalid("CPX export requires DOS FONT data"));
    }
    check(n, o.limits.max_decompressed_bytes, "CPX decoded bytes")?;
    check(n, o.limits.max_table_bytes, "CPX decoded table")?;
    // One literal costs one byte and one control bit. Match tokens are emitted
    // only for >=3 bytes, so this also bounds their maximum wire cost.
    let bound = n + (n + 64).div_ceil(16) * 2 + 16;
    budget.reserve(
        bound
            .saturating_mul(3)
            .saturating_add(2 * (ENTRY + stub::DECODER.len()))
            .saturating_add(2 * (4096 + 256))
            .saturating_add(n)
            .saturating_add(2048),
    )?;
    let mut stream = Stream {
        bytes: Vec::with_capacity(bound),
        word: 0,
        used: 0,
    };
    let mut heads = [EMPTY; 4096];
    let mut previous = [EMPTY; 256];
    let mut pos = 0usize;
    let mut last_distance = 1usize;
    let mut max_delta = 0usize;
    while pos < n {
        let (mut length, mut distance) = (0usize, 0usize);
        if pos >= 3 && pos + 2 < n {
            let mut candidate = heads[hash(&cpi, pos)];
            for _ in 0..64 {
                let at = usize::from(candidate);
                if candidate == EMPTY || at >= pos || pos - at > 256 {
                    break;
                }
                // MODE changes decoded byte zero to RETF. Never reference it,
                // including indirectly through a later copied match.
                if at == 0 {
                    break;
                }
                let mut count = 0;
                while pos + count < n && cpi[at + count] == cpi[pos + count] {
                    count += 1;
                }
                if count > length {
                    length = count;
                    distance = pos - at;
                    if pos + count == n {
                        break;
                    }
                }
                candidate = previous[at % 256];
            }
        }
        let consumed = if length >= 3 {
            stream.bit(false);
            if distance == last_distance {
                stream.number(2);
            } else {
                stream.number(3);
                stream.bytes.push((distance - 1) as u8);
                last_distance = distance;
            }
            if length <= 4 {
                stream.bit((length - 1) & 2 != 0);
                stream.bit((length - 1) & 1 != 0);
            } else {
                stream.bit(false);
                stream.bit(false);
                stream.number((length - 3) as u32);
            }
            length
        } else {
            stream.bit(true);
            stream.bytes.push(cpi[pos]);
            1
        };
        let end = pos + consumed;
        max_delta = max_delta.max(end.saturating_sub(stream.bytes.len()));
        while pos < end {
            if pos + 2 < n {
                let bucket = hash(&cpi, pos);
                previous[pos % 256] = heads[bucket];
                heads[bucket] = pos as u16;
            }
            pos += 1;
        }
    }
    stream.bit(false);
    stream.number(0x0100_0002);
    stream.bytes.push(255);
    let packed = stream.bytes.len();
    if packed > n {
        return Err(invalid(
            "CPX input is not compressible by the bounded encoder",
        ));
    }
    // The input pointer already consumed each token before its output is
    // written. Keep the output below that unread-input boundary throughout.
    let overlap = ENTRY.max(max_delta.saturating_add(packed).saturating_sub(n));
    if overlap + stub::DECODER.len() > MODE_EXTRA {
        return Err(invalid(
            "CPX overlap exceeds FreeDOS MODE's 320-byte work area",
        ));
    }
    let code_end = ENTRY + packed + stub::DECODER.len();
    // UPX's top-level input reader rejects any file shorter than 512 bytes.
    // Trailing zero fill lies outside both the declared stream and executable.
    let size = code_end.max(512);
    if size > MODE_INPUT {
        return Err(invalid(
            "CPX file exceeds FreeDOS MODE's 58880-byte input limit",
        ));
    }
    check(size, o.max_output_bytes, "CPX output bytes")?;
    check(size, o.limits.max_input_bytes, "CPX input bytes")?;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(stub::BOOTSTRAP);
    let decoder_at = 0x100 + n + overlap;
    let relocated_end = decoder_at + stub::DECODER.len();
    put_word(&mut out, 2, relocated_end + 0x60)?;
    put_word(&mut out, 9, packed + stub::DECODER.len())?;
    put_word(&mut out, 12, 0x100 + code_end - 1)?;
    put_word(&mut out, 15, relocated_end - 1)?;
    put_word(&mut out, 22, decoder_at - packed)?;
    put_word(&mut out, 35, decoder_at)?;
    out.extend_from_slice(b"UPX!");
    out.extend_from_slice(&[11, 1, 4, 1]);
    out.extend_from_slice(&super::cpx::adler(&cpi).to_le_bytes());
    out.extend_from_slice(&super::cpx::adler(&stream.bytes).to_le_bytes());
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&(packed as u16).to_le_bytes());
    out.push(0); // No executable call filter.
    let checksum = out[stub::BOOTSTRAP.len() + 4..]
        .iter()
        .map(|&v| usize::from(v))
        .sum::<usize>()
        % 251;
    out.push(checksum as u8);
    out.extend_from_slice(&stream.bytes);
    out.extend_from_slice(stub::DECODER);
    out.resize(size, 0);
    Ok(out)
}
