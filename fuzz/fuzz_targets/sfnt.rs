#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{ResourceLimits,sfnt::{Limits,Sfnt,SfntBuilder,cmap,name::NameTable,post::PostTable,os2::Os2Table,gasp::GaspTable,metrics::{self,Axis}}};
fuzz_target!(|data:&[u8]| {
    let limits=Limits{resources:ResourceLimits{max_input_bytes:65536,max_total_bytes:1024*1024,max_table_bytes:65536,max_glyphs:256,..Default::default()},max_tables:64,max_cmap_mappings:4096,max_name_records:256};
    if let Ok(f)=Sfnt::parse(data,&limits) {
        let _=f.validate_checksums();
        let mut b=SfntBuilder::new(f.flavor());for t in f.tables(){b.insert(t.tag,t.data.to_vec()).unwrap();}
        if let Ok(out)=b.build(&limits) {let _=Sfnt::parse(&out,&limits);}
        if let Some(table)=f.table(*b"name"){let _=NameTable::decode(table,&limits);}
        if let Some(table)=f.table(*b"cmap"){let _=cmap::decode(table,256,&limits);}
    }
    if let Ok(m)=cmap::decode(data,256,&limits){if let Ok(e)=cmap::encode(&m,256,&limits){assert_eq!(cmap::decode(&e.data,256,&limits).unwrap(),m);}}
    if let Ok(n)=NameTable::decode(data,&limits){if let Ok(e)=n.encode(&limits){let re=NameTable::decode(&e,&limits).unwrap();assert_eq!(re.encode(&limits).unwrap(),e);}}
    if let Ok(o)=Os2Table::decode(data,&limits){if let Ok(e)=o.encode(&limits){assert_eq!(Os2Table::decode(&e,&limits).unwrap(),o);}}
    if let Ok(p)=PostTable::decode(data,4,&limits){if let Ok(e)=p.encode(4,&limits){assert_eq!(PostTable::decode(&e,4,&limits).unwrap(),p);}}
    if let Ok(g)=GaspTable::decode(data,&limits){if let Ok(e)=g.encode(&limits){assert_eq!(GaspTable::decode(&e,&limits).unwrap(),g);}}
    for axis in [Axis::Horizontal,Axis::Vertical]{let _=metrics::decode(axis,data,data,4,&limits);}
});
