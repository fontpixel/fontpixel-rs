//! Exact semantics and charge boundaries for allocation-light BDF parsing.
#![cfg(feature = "bdf")]
use fontpixel::formats::bdf;
use fontpixel::model::{Codepoint, Property, PropertyValue, Vector};
use fontpixel::report::{Impact, Level, Location};
use fontpixel::{Error, ReportEntry, ResourceLimits};

const HEAD: &str = "STARTFONT 2.1\nFONT hot\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 2 0 0\nSTARTPROPERTIES 0\nENDPROPERTIES\nCHARS 1\n";
const GLYPH: &str = "STARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 8 2 0 0\nBITMAP\nFF\n00\nENDCHAR\n";

fn source(glyph: &str) -> String { format!("{HEAD}{glyph}ENDFONT\n") }
fn assert_parse_error(input: &str, bad_line: &str, expected: &str) {
    let expected_line = input.lines().position(|line| line == bad_line).unwrap() + 1;
    let error = bdf::read(input.as_bytes(), &ResourceLimits::default()).unwrap_err();
    match error {
        Error::BdfParse { line, message } => { assert_eq!(line, expected_line, "{bad_line}"); assert_eq!(message, expected, "{bad_line}"); }
        other => panic!("{bad_line}: expected BdfParse, got {other:?}"),
    }
}
#[test]
fn numeric_errors_keep_count_precedence_exact_text_and_line() {
    for (old,new,expected) in [
        ("DWIDTH 8 0", "DWIDTH", "DWIDTH needs 2 numbers, got ''"),
        ("DWIDTH 8 0", "DWIDTH bad", "DWIDTH needs 2 numbers, got 'bad'"),
        ("DWIDTH 8 0", "DWIDTH bad 0", "malformed number in DWIDTH 'bad 0'"),
        ("DWIDTH 8 0", "DWIDTH 2147483648 0", "number 2147483648 in DWIDTH is out of the 32-bit range"),
        ("BBX 8 2 0 0", "BBX bad 0 0", "BBX needs 4 numbers, got 'bad 0 0'"),
        ("BBX 8 2 0 0", "BBX bad 0 0 0", "malformed number in BBX 'bad 0 0 0'"),
        ("BBX 8 2 0 0", "BBX -1 2 0 0", "glyph 'A' has a negative BBX size -1 2"),
        ("ENCODING 65", "ENCODING", "ENCODING needs a number"),
        ("ENCODING 65", "ENCODING bad 2147483648", "malformed number in ENCODING 'bad 2147483648'"),
        ("ENCODING 65", "ENCODING 65 bad", "malformed number in ENCODING '65 bad'"),
        ("ENCODING 65", "ENCODING -2147483649", "number -2147483649 in ENCODING is out of the 32-bit range"),
    ] { assert_parse_error(&source(&GLYPH.replace(old,new)),new,expected); }
    for (old,new,expected) in [
        ("SIZE 8 75 75", "SIZE bad 75", "SIZE needs 3 numbers, got 'bad 75'"),
        ("SIZE 8 75 75", "SIZE bad 75 75", "malformed number in SIZE 'bad 75 75'"),
        ("CHARS 1", "CHARS", "CHARS needs 1 numbers, got ''"),
    ] { assert_parse_error(&source(GLYPH).replace(old,new),new,expected); }
}
#[test]
fn vector_comma_edges_keep_reference_tokenization() {
    for (value,expected) in [
        ("", "VVECTOR needs 2 numbers, got ''"), ("bad", "VVECTOR needs 2 numbers, got 'bad'"),
        (",", "malformed number in VVECTOR ','"), (",1 2", "malformed number in VVECTOR ',1 2'"),
        ("1,", "malformed number in VVECTOR '1,'"), ("1,bad", "malformed number in VVECTOR '1,bad'"),
    ] {
        let line = format!("VVECTOR{separator}{value}",separator=if value.is_empty(){""}else{" "});
        let input=source(&GLYPH.replace("BITMAP",&format!("{line}\nBITMAP")));
        assert_parse_error(&input,&line,expected);
    }
    for value in ["1,2,", "1,,2", "+1,\t-2 ignored", "1\u{b}2\u{c}ignored"] {
        let input=source(&GLYPH.replace("BITMAP",&format!("VVECTOR {value}\nBITMAP")));
        let (font,_)=bdf::read(input.as_bytes(),&ResourceLimits::default()).unwrap();
        let glyph=font.glyphs().next().unwrap().1;
        assert_eq!(glyph.metrics.vertical_origin,Some(Vector::new(1,if value.contains("-2"){-2}else{2})));
    }
}
#[test]
fn extra_tokens_are_ignored_and_header_lists_keep_exact_arity() {
    let input=source(GLYPH).replace("SIZE 8 75 75","SIZE 8 75 75 malformed").replace("FONTBOUNDINGBOX 8 2 0 0","FONTBOUNDINGBOX 8 2 0 0 malformed").replace("STARTPROPERTIES 0","DWIDTH 7 0 ignored\nSTARTPROPERTIES 0").replace("CHARS 1","CHARS 1 ignored").replace("ENCODING 65","ENCODING 65 99 malformed").replace("DWIDTH 8 0","DWIDTH 8 0 malformed").replace("BBX 8 2 0 0","BBX 8 2 0 0 malformed");
    let(font,_)=bdf::read(input.as_bytes(),&ResourceLimits::default()).unwrap();
    for (key,count) in [("SIZE",3),("FONTBOUNDINGBOX",4),("DWIDTH",2)] {
        let Some(PropertyValue::List(values))=font.headers.get(key) else {panic!("missing {key}")};
        assert_eq!(values.len(),count,"{key}");
    }
    let glyph=font.glyphs().next().unwrap().1;
    assert_eq!(glyph.codepoints,[Codepoint::Unicode(65)]);
    assert_eq!(glyph.metrics.advance,Some(Vector::new(8,0)));
    assert!(glyph.properties.get("bdf.altcode").is_none());
}
#[test]
fn fixed_flags_preserve_all_duplicate_fields_and_first_values() {
    let glyph="STARTCHAR A\nENCODING 65\nENCODING not-a-number\nBBX 8 2 0 0\nBBX duplicate\nSWIDTH 100 2\nSWIDTH duplicate\nDWIDTH 8 0\nDWIDTH duplicate\nSWIDTH1 3 4\nSWIDTH1 duplicate\nDWIDTH1 5 6\nDWIDTH1 duplicate\nVVECTOR 7,8\nVVECTOR duplicate\nBITMAP\nFF\n00\nENDCHAR\n";
    let(font,report)=bdf::read(source(glyph).as_bytes(),&ResourceLimits::default()).unwrap();
    assert!(!report.entries.iter().any(|entry|entry.code.starts_with("bdf.parse.")));
    let glyph=font.glyphs().next().unwrap().1;
    assert_eq!(glyph.codepoints,[Codepoint::Unicode(65)]);
    assert_eq!((glyph.metrics.bounds.width,glyph.metrics.bounds.height),(8,2));
    assert_eq!(glyph.metrics.scalable_advance,Some(Vector::new(100,2)));
    assert_eq!(glyph.metrics.advance,Some(Vector::new(8,0)));
    assert_eq!(glyph.metrics.vertical_scalable_advance,Some(Vector::new(3,4)));
    assert_eq!(glyph.metrics.vertical_advance,Some(Vector::new(5,6)));
    assert_eq!(glyph.metrics.vertical_origin,Some(Vector::new(7,8)));
    let duplicates:Vec<_>=glyph.properties.iter().filter(|p|p.name.starts_with("bdf.duplicate.")).collect();
    assert_eq!(duplicates.len(),7);
    for (property,key) in duplicates.iter().zip(["ENCODING","BBX","SWIDTH","DWIDTH","SWIDTH1","DWIDTH1","VVECTOR"]) {
        assert_eq!(property.name,format!("bdf.duplicate.{key}"));
        assert_eq!(property.value,PropertyValue::Text(if key=="ENCODING"{"not-a-number"}else{"duplicate"}.into()));
        assert!(property.raw.is_none());
    }
}
fn bitmap_entry(code:&str,message:&str,index:usize,cp:u32)->ReportEntry {
    ReportEntry {level:Level::Info,impact:Impact::Repair,code:format!("bdf.bitmap.{code}"),location:Location{font_index:Some(0),glyph_index:Some(index),codepoint:Some(cp),asset:None},message:message.into()}
}
#[test]
fn lazy_repairs_keep_exact_order_locations_and_source_rows() {
    let glyphs="STARTCHAR A\nENCODING 65\nBBX 9 5 0 0\nBITMAP\nF\nFFFFF\nFFFF\nXX\nSTARTCHAR B\nENCODING 66\nBBX 1 1 0 0\nBITMAP\n80\nFF\nENDCHAR\n";
    let input=source(glyphs).replace("CHARS 1","CHARS 2");
    let(font,report)=bdf::read(input.as_bytes(),&ResourceLimits::default()).unwrap();
    let repairs:Vec<_>=report.entries.iter().filter(|e|e.code.starts_with("bdf.bitmap.")).cloned().collect();
    assert_eq!(repairs,[
        bitmap_entry("missing_rows","Glyph \"A\" (codepoint 65): 4 source rows for height 5; zero-padded core view, original rows retained",0,65),
        bitmap_entry("short_row","Short bitmap rows zero-padded in the core rectangle; source rows retained",0,65),
        bitmap_entry("long_row","Long bitmap rows clipped in the core rectangle; source rows retained",0,65),
        bitmap_entry("padding","Nonzero right padding cleared in the core rectangle; source rows retained",0,65),
        bitmap_entry("invalid_hex","Non-hex bitmap rows retained; core placeholder rows are zero. validate_bitmap rejects drawing this glyph",0,65),
        bitmap_entry("extra_rows","Extra bitmap rows excluded from the core rectangle; source rows retained",1,66),
    ]);
    let missing=&report.entries[0];
    let line=input.lines().position(|line|line=="STARTCHAR B").unwrap()+1;
    assert_eq!(missing.code,"bdf.parse.missing_endchar");
    assert_eq!(missing.message,format!("line {line}: missing ENDCHAR for glyph 'A'"));
    assert_eq!(missing.location,Location{font_index:Some(0),glyph_index:Some(0),codepoint:Some(65),asset:None});
    let a=font.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(bdf::source_rows(a).unwrap().collect::<Vec<_>>(),["F","FFFFF","FFFF","XX"]);
    assert_eq!(a.bitmap.as_bytes(),&[0xf0,0,0xff,0x80,0xff,0x80,0,0,0,0]);
    assert_eq!(bdf::validate_bitmap(a).unwrap_err().to_string(),"Glyph \"A\" (codepoint 65): bitmap row 3 \"XX\" contains a non-hex character");
    let output=String::from_utf8(bdf::write(&font,&Default::default()).unwrap().0).unwrap();
    assert!(output.contains("BITMAP\nF\nFFFFF\nFFFF\nXX\nENDCHAR\n"));
}
#[test]
fn missing_bbx_error_precedes_encoding_and_bitmap_repairs() {
    let input=source("STARTCHAR A\nBITMAP\nXX\nENDCHAR\n");
    let start=input.lines().position(|line|line=="STARTCHAR A").unwrap()+1;
    assert_parse_error(&input,"ENDCHAR",&format!("glyph 'A' (starting at line {start}) has no BBX"));
}
#[test]
fn charset_memo_uses_completed_properties_including_recovered_boundaries() {
    for(registry,encoding,unicode)in[("ISO10646","1",true),("UTF_16","1",true),("unknown-test-charset","0",true),("ISO8859","1",false),("SJIS","0",false),("GB2312.1980","0",false)] {
        let properties=format!("STARTPROPERTIES 3\nCHARSET_REGISTRY \"ISO10646\"\nCHARSET_REGISTRY \"{registry}\"\nCHARSET_ENCODING \"{encoding}\"\nENDPROPERTIES");
        let glyphs=format!("{}{}{}",GLYPH.replace("ENCODING 65","ENCODING -1"),GLYPH,GLYPH.replace("ENCODING 65","ENCODING 66"));
        let normal=source(&glyphs).replace("STARTPROPERTIES 0\nENDPROPERTIES",&properties).replace("CHARS 1","CHARS 3");
        for input in [normal.clone(),normal.replace("ENDPROPERTIES\n",""),normal.replace("ENDPROPERTIES\nCHARS 3\n","")] {
            let(font,_)=bdf::read(input.as_bytes(),&ResourceLimits::default()).unwrap();
            let glyphs:Vec<_>=font.glyphs().map(|(_,g)|g).collect();
            assert!(glyphs[0].codepoints.is_empty());
            for(glyph,code)in glyphs[1..].iter().zip([65,66]) {assert_eq!(glyph.codepoints,[if unicode{Codepoint::Unicode(code)}else{Codepoint::Legacy(code)}],"{registry}");}
        }
    }
}
// Keep the pre-optimization accounting constants, including fixed overhead
// despite fewer temporary allocations. This is independent of parser internals.
fn charged_bytes(rows:&str,messages:&[&str])->usize {
    let headers:usize=HEAD.lines().take(4).map(|line|std::mem::size_of::<Property>()+line.split(' ').next().unwrap().len()+line.len()*2+64).sum();
    headers+512+1+rows.len()+2*3+256+messages.iter().map(|message|message.len()+160).sum::<usize>()
}
#[test]
fn model_charge_boundaries_are_unchanged_for_clean_and_repaired_rows() {
    for(rows,messages)in[
        ("FF\n00\n",vec![]),
        ("FF\n",vec!["Glyph \"A\" (codepoint 65): 1 source rows for height 2; zero-padded core view, original rows retained"]),
        ("XX\n00\n",vec!["Non-hex bitmap rows retained; core placeholder rows are zero. validate_bitmap rejects drawing this glyph"]),
    ] {
        let input=source(&GLYPH.replace("FF\n00\n",rows)); let exact=charged_bytes(rows,&messages); assert!(input.len()<exact);
        for limit in [exact-1,exact] {
            let result=bdf::read(input.as_bytes(),&ResourceLimits{max_total_bytes:limit,..Default::default()});
            if limit==exact {assert!(result.is_ok(),"at exact charge {exact}: {result:?}");} else {assert!(matches!(result,Err(Error::ResourceLimit{resource:"BDF model bytes",limit:n})if n==limit),"below exact charge {exact}: {result:?}");}
        }
    }
}
#[test]
fn glyph_pixel_line_and_input_caps_remain_enforced_at_boundaries() {
    let input=source(GLYPH);
    for limits in [
        ResourceLimits{max_glyphs:1,..Default::default()},ResourceLimits{max_pixels_per_glyph:16,..Default::default()},
        ResourceLimits{max_input_bytes:input.len(),..Default::default()},ResourceLimits{max_table_bytes:input.split_inclusive('\n').map(str::len).max().unwrap(),..Default::default()},
    ] {assert!(bdf::read(input.as_bytes(),&limits).is_ok());}
    for(limits,resource)in[
        (ResourceLimits{max_glyphs:0,..Default::default()},"glyph count"),
        (ResourceLimits{max_pixels_per_glyph:15,..Default::default()},"bitmap pixels"),
        (ResourceLimits{max_input_bytes:input.len()-1,..Default::default()},"BDF input bytes"),
        (ResourceLimits{max_table_bytes:input.split_inclusive('\n').map(str::len).max().unwrap()-1,..Default::default()},"BDF line bytes"),
    ] {
        let result=bdf::read(input.as_bytes(),&limits);
        assert!(matches!(result,Err(Error::ResourceLimit{resource:name,..})if name==resource),"{resource}: {result:?}");
    }
}

// Leading ASCII whitespace deliberately forces the unchanged lexical path,
// while the existing trim rule leaves exactly the same retained bitmap row.
fn indented_hex_rows(input:&str)->String {
    input.split_inclusive('\n').map(|raw|{
        let row=raw.strip_suffix('\n').unwrap_or(raw);
        if !row.is_empty()&&row.bytes().all(|b|b.is_ascii_hexdigit()){format!(" \t{raw}")}else{raw.into()}
    }).collect()
}
fn assert_row_paths_match(input:&str) {
    let fallback=indented_hex_rows(input);
    let expected=bdf::read(fallback.as_bytes(),&ResourceLimits::default()).unwrap();
    let expected_wire=bdf::write(&expected.0,&Default::default()).unwrap();
    for separator in ["\n","\r\n","\r"] {
        for candidate in [input,fallback.as_str()] {
            let candidate=candidate.replace('\n',separator);
            let actual=bdf::read(candidate.as_bytes(),&ResourceLimits::default()).unwrap();
            assert_eq!(actual,expected,"line separator {separator:?}");
            assert_eq!(bdf::write(&actual.0,&Default::default()).unwrap(),expected_wire);
        }
    }
}
#[test]
fn bitmap_row_fast_path_preserves_bytes_reports_and_line_endings() {
    for rows in ["FF\n00\n","ff\n0\n","FFFF\n00\n","FF\n00\nFF\n","\n \t\u{b}\u{c}\nFF\n\n00\n"," \tFF\u{b}\u{c}\n 00 \n","FF\nXX\n","FF\n0x00\n"] {
        assert_row_paths_match(&source(&GLYPH.replace("FF\n00\n",rows)));
    }
}
#[test]
fn unicode_whitespace_rows_remain_malformed_retained_source() {
    for row in ["\u{a0}FF\u{a0}","\u{2003}FF\u{2003}","\u{a0}"] {
        let input=source(&GLYPH.replace("FF\n00\n",&format!("{row}\n00\n")));
        assert_row_paths_match(&input);
        let(font,report)=bdf::read(input.as_bytes(),&ResourceLimits::default()).unwrap();
        let glyph=font.glyphs().next().unwrap().1;
        assert_eq!(bdf::source_rows(glyph).unwrap().collect::<Vec<_>>(),[row,"00"]);
        assert_eq!(glyph.bitmap.as_bytes(),&[0,0]);
        let repairs:Vec<_>=report.entries.iter().filter(|e|e.code.starts_with("bdf.bitmap.")).cloned().collect();
        assert_eq!(repairs,[bitmap_entry("invalid_hex","Non-hex bitmap rows retained; core placeholder rows are zero. validate_bitmap rejects drawing this glyph",0,65)]);
    }
}
#[test]
fn bitmap_fast_rows_do_not_bypass_comments_or_control_transitions() {
    let comment=source(&GLYPH.replace("FF\n00\n","FF\nCOMMENT preserved comment\n00\n"));
    assert_row_paths_match(&comment);
    let(font,_)=bdf::read(comment.as_bytes(),&ResourceLimits::default()).unwrap();
    let glyph=font.glyphs().next().unwrap().1;
    assert_eq!(glyph.properties.get("bdf.bitmap.comment"),Some(&PropertyValue::Text("preserved comment".into())));
    assert_eq!(bdf::source_rows(glyph).unwrap().collect::<Vec<_>>(),["FF","00"]);
    let next=GLYPH.replace("STARTCHAR A","STARTCHAR B").replace("ENCODING 65","ENCODING 66");
    let missing_endchar=source(&format!("{}{next}",GLYPH.replace("ENDCHAR\n",""))).replace("CHARS 1","CHARS 2");
    assert_row_paths_match(&missing_endchar);
    let(_,report)=bdf::read(missing_endchar.as_bytes(),&ResourceLimits::default()).unwrap();
    assert_eq!(report.entries[0].code,"bdf.parse.missing_endchar");
    assert_row_paths_match(&source(&GLYPH.replace("ENDCHAR\n","")));
}
#[test]
fn final_unterminated_hex_row_keeps_eof_repairs_and_missing_bbx_precedence() {
    let eof=format!("{HEAD}{}",GLYPH.replace("ENDCHAR\n",""));
    for input in [eof.as_str(),eof.trim_end_matches('\n')] {
        assert_row_paths_match(input);
        let(_,report)=bdf::read(input.as_bytes(),&ResourceLimits::default()).unwrap();
        assert_eq!(report.entries[0].code,"bdf.parse.missing_endchar");
        assert_eq!(report.entries[0].message,"Unexpected end of file: missing ENDCHAR for glyph 'A'");
        assert_eq!(report.entries[1].code,"bdf.parse.missing_endfont");
    }
    let missing=source(&GLYPH.replace("BBX 8 2 0 0\n",""));
    for separator in ["\n","\r\n"] {for input in [missing.clone(),indented_hex_rows(&missing)] {
        let input=input.replace('\n',separator);
        assert_parse_error(&input,"ENDCHAR","glyph 'A' (starting at line 8) has no BBX");
    }}
}
#[test]
fn bitmap_fast_rows_preserve_late_source_limit_and_charge_precedence() {
    let rows="FF\n".repeat(32);
    let input=source(&GLYPH.replace("BBX 8 2 0 0","BBX 8 32 0 0").replace("FF\n00\n",&rows));
    let initial=charged_bytes("",&[])-(2*3+256);
    // The first 21 rows fit in 63 bytes. The 22nd row is charged before
    // the 64-byte source-row cap, even when both bounds would fail there.
    for(total,resource)in[(initial+65,"BDF model bytes"),(initial+66,"BDF glyph source rows")] {
        for separator in ["\n","\r\n"] {for input in [input.clone(),indented_hex_rows(&input)] {
            let input=input.replace('\n',separator);
            let error=bdf::read(input.as_bytes(),&ResourceLimits{max_total_bytes:total,max_table_bytes:64,..Default::default()}).unwrap_err();
            assert!(matches!(error,Error::ResourceLimit{resource:actual,..}if actual==resource),"{error:?}");
        }}
    }
    let oversized=source(&GLYPH.replace("FF\n00\n",&format!("FF\n00\n{}\n","F".repeat(65))));
    let error=bdf::read(oversized.as_bytes(),&ResourceLimits{max_total_bytes:initial+6,max_table_bytes:64,..Default::default()}).unwrap_err();
    assert!(matches!(error,Error::ResourceLimit{resource:"BDF line bytes",limit:64}),"raw line check must precede the exhausted row charge: {error:?}");
}
