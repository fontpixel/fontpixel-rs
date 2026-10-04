// Original project-authored fixtures, MIT. No external font or program bytes.
fn fixture(id: &str) -> Vec<u8> {
    match id {
        "dosstart" => {let mut s="DosStartFont\nOriginal\n1, 2\n".to_owned();for _ in 32..128{s.push_str("003002HGHGHH, 4\n");}s.into_bytes()},
        "edwin" => b"65 66\n65: 2 3 5 6\nbee: 1 3 3\n".to_vec(),
        "mkwinfont" => b"facename Original\nheight 2\nascent 1\npointsize 9\ncharset 0\n# font comment\nchar 65\nwidth 3\n101\n011\nchar 66\nwidth 2\n10\n11\n".to_vec(),
        "psf2txt" => b"%PSF2\nVersion: 0\nFlags: 1\nLength: 2\nWidth: 3\nHeight: 2\n%\nBitmap: #-# \\\n        -##\nUnicode: [0041];[0391];[0061+0301];\n%\nBitmap: ##- \\\n        #--\n".to_vec(),
        "pcr" => {let mut b=b"KPG\x01\x02\x20\x01\x02\0\0\x02".to_vec();for _ in 0..256{b.extend_from_slice(&[0xa5,0x63]);}b},
        "plus3dos"=>{let mut b=vec![0;896];b[..9].copy_from_slice(b"PLUS3DOS\x1a");b[9]=1;b[11..15].copy_from_slice(&896u32.to_le_bytes());b[15]=3;b[16..18].copy_from_slice(&768u16.to_le_bytes());b[18..20].copy_from_slice(&0x3d00u16.to_le_bytes());b[22]=b'f';b[127]=b[..127].iter().fold(0u8,|a,&v|a.wrapping_add(v));for v in &mut b[128..]{*v=0xa5;}b},
        "drhalo"=>{let mut b=vec![0;20];b[..2].copy_from_slice(b"AH");b[2]=0xdf;b[4]=0x0a;b[6..8].copy_from_slice(&0x300u16.to_le_bytes());b[10]=127;b[12]=4;b[14]=2;b[16]=9;b[18]=2;b.extend_from_slice(&[0xa5,0x80,0x63,0,0x80,0,0,0x80]);b},
        "writeon"=>{let mut b=vec![0;22];b[..4].copy_from_slice(b"ES\x01\0");b[4]=22;b[6]=8;b[8]=14;b[10]=14;b[12]=65;b[14]=66;b[18]=8;b.extend_from_slice(&[0xa5;28]);b},
        _=>unreachable!()
    }
}
