#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::sfnt::gsub::{Gsub,LayoutLimits};
use std::collections::{BTreeSet,BTreeMap};
fuzz_target!(|data:&[u8]| {
    let mut l=LayoutLimits::default();l.sfnt.resources.max_table_bytes=65536;l.max_records=8192;
    let count=256;
    if let Ok(g)=Gsub::decode(data,count,&l){
        let _=g.encode(count,&l);
        let mut keep=BTreeSet::from([0,1,65]);g.closure(&mut keep);
        let map=keep.iter().enumerate().map(|(i,&g)|(g,i as u16)).collect::<BTreeMap<_,_>>();
        let _=g.subset(&map).encode(keep.len() as u16,&l);
    }
});
