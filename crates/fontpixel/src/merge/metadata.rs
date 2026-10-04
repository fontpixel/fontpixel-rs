//! Reconstructed W6.5 metadata implementation; currently untested.
use super::{Budget, MergeOptions, MergePart, ModificationPermission, diagnostic};
use super::report::LicenseReport;
use crate::{Error, Report, Result};
use crate::report::Impact;
use crate::model::{Font, PropertyValue};
use std::collections::BTreeSet;

fn property_permission(font:&Font)->ModificationPermission {
    let mut permission=ModificationPermission::Unknown;
    for name in ["MODIFICATION_ALLOWED","ALLOW_MODIFICATION"] {
        let declaration=match font.properties.get_case_insensitive(name) {
            Some(PropertyValue::Text(text))=>match text.trim().to_ascii_lowercase().as_str() {
                "true"|"yes"|"1"=>ModificationPermission::Allowed,
                "false"|"no"|"0"=>ModificationPermission::Prohibited,
                _=>ModificationPermission::Unknown,
            },
            Some(PropertyValue::Integer(0)|PropertyValue::Unsigned(0))=>ModificationPermission::Prohibited,
            Some(PropertyValue::Integer(1)|PropertyValue::Unsigned(1))=>ModificationPermission::Allowed,
            _=>ModificationPermission::Unknown,
        };
        if declaration==ModificationPermission::Prohibited{return declaration;}
        if declaration==ModificationPermission::Allowed{permission=declaration;}
    }
    permission
}
fn combine(primary:Option<&str>,properties:impl Iterator<Item=String>)->Option<String> {
    let mut values=Vec::new();let mut seen=BTreeSet::new();
    if let Some(s)=primary.filter(|s|!s.trim().is_empty()){seen.insert(s.to_owned());values.push(s.to_owned());}
    for value in properties{if !value.trim().is_empty()&&seen.insert(value.clone()){values.push(value);}}
    if values.is_empty(){None}else{Some(values.join("\n"))}
}
fn source_text(font:&Font,primary:Option<&str>,keys:&[&str])->Option<String>{
    combine(primary,font.properties.iter().filter(|p|keys.iter().any(|k|p.name.eq_ignore_ascii_case(k))).filter_map(|p|match &p.value{PropertyValue::Text(t)=>Some(t.clone()),_=>None}))
}
fn identified_license(value:&str)->Option<&'static str>{
    let mut found=None;
    for line in value.split(['\n',';']){
        let exact=line.trim().trim_end_matches('.').to_ascii_lowercase();
        let id=match exact.as_str(){
            "ofl-1.1"|"sil open font license 1.1"|"sil open font license version 1.1"|"sil open font license, version 1.1"|"sil open font license version 1.1 - 26 february 2007"|"sil open font license, version 1.1 - 26 february 2007"=>Some("OFL-1.1"),
            "mit"|"mit license"=>Some("MIT"),
            "apache-2.0"|"apache license, version 2.0"=>Some("Apache-2.0"),
            "bsd-2-clause"=>Some("BSD-2-Clause"),"bsd-3-clause"=>Some("BSD-3-Clause"),"cc0-1.0"=>Some("CC0-1.0"),
            "cc-by-nd-4.0"=>return Some("CC-BY-ND-4.0"),
            "cc-by-nc-nd-4.0"=>return Some("CC-BY-NC-ND-4.0"),
            _=>None,
        };
        if found.is_none(){found=id;}
    }
    found
}
fn parse_reserved(text:&str,result:&mut Vec<String>,maximum:usize)->Result<()>{
    for line in text.split(['\n',';']){
        let trimmed=line.trim();let lower=trimmed.to_ascii_lowercase();
        let suffix=lower.strip_prefix("reserved font names").map(|s|&trimmed[trimmed.len()-s.len()..])
            .or_else(||lower.strip_prefix("reserved font name").map(|s|&trimmed[trimmed.len()-s.len()..]));
        if let Some(suffix)=suffix{
            let suffix=suffix.trim_start_matches([' ',':','=']);
            if suffix.is_empty()||suffix.to_ascii_lowercase().starts_with("refers ")||suffix.starts_with("\" refers"){continue;}
            for name in suffix.split(',').map(|n|n.trim().trim_matches(['\'','"']).trim()){
                if !name.is_empty()&&!result.iter().any(|s|s.eq_ignore_ascii_case(name)){
                    if result.len()>=maximum{return Err(Error::ResourceLimit{resource:"merge reserved names",limit:maximum});}
                    result.push(name.into());
                }
            }
        }
    }
    Ok(())
}
pub(super) fn licenses(parts:&[MergePart<'_>],options:&MergeOptions,report:&mut Report,budget:&mut Budget)->Result<Vec<LicenseReport>>{
    let mut result=Vec::new();
    for (index,part) in parts.iter().enumerate(){
        let label=part.label_text(index);let original=part.font;
        let mut copyright=source_text(original,original.names.copyright.as_deref(),&["COPYRIGHT"]);
        let mut license=source_text(original,original.names.license.as_deref(),&["LICENSE","LICENSE_TEXT","SPDX_LICENSE_IDENTIFIER"]);
        let mut license_url=source_text(original,original.names.license_url.as_deref(),&["LICENSE_URL"]);
        copyright=combine(copyright.as_deref(),part.license.copyright.clone().into_iter());
        license=combine(license.as_deref(),part.license.license.clone().into_iter());
        license_url=combine(license_url.as_deref(),part.license.license_url.clone().into_iter());
        let license_id=license.as_deref().and_then(identified_license);
        let copyright_id=copyright.as_deref().and_then(identified_license);
        let identifier=[license_id,copyright_id].into_iter().flatten()
            .find(|id|matches!(*id,"CC-BY-ND-4.0"|"CC-BY-NC-ND-4.0")).or(license_id).or(copyright_id);
        if license.is_none(){license=identifier.map(str::to_owned);}
        let mut modification=part.license.modification;let mut provenance="caller permission override".to_owned();
        if modification==ModificationPermission::Unknown{
            modification=property_permission(original);provenance="explicit modification property".into();
            if modification==ModificationPermission::Unknown{
                if let Some(id)=identifier{
                    modification=if matches!(id,"CC-BY-ND-4.0"|"CC-BY-NC-ND-4.0"){ModificationPermission::Prohibited}else{ModificationPermission::Allowed};
                    provenance=format!("exact standard identifier/title: {id}");
                }else{provenance="no recognized explicit license/permission declaration".into();}
            }
        }
        let mut reserved_names=part.license.reserved_names.clone();
        for p in original.properties.iter().filter(|p|["RESERVED_FONT_NAMES","RESERVED_FONT_NAME"].iter().any(|k|p.name.eq_ignore_ascii_case(k))){
            match &p.value{
                PropertyValue::Text(t)=>parse_reserved(&format!("Reserved Font Names: {t}"),&mut reserved_names,options.limits.max_resources)?,
                PropertyValue::List(values)=>for v in values{if let PropertyValue::Text(t)=v{
                    if reserved_names.len()>=options.limits.max_resources{return Err(Error::ResourceLimit{resource:"merge reserved names",limit:options.limits.max_resources});}
                    reserved_names.push(t.clone());
                }},
                _=>{}
            }
        }
        for value in [&copyright,&license].into_iter().flatten(){parse_reserved(value,&mut reserved_names,options.limits.max_resources)?;}
        for name in &mut reserved_names{*name=name.trim().trim_matches(['\'','"']).trim().to_owned();}
        reserved_names.retain(|n|!n.is_empty());let family=options.family.to_lowercase();
        for name in &reserved_names{if family.contains(&name.to_lowercase()){return Err(Error::InvalidInput(format!("new family {:?} contains reserved font name {:?} from part {index}",options.family,name)));}}
        if modification!=ModificationPermission::Allowed||license.as_deref().is_none_or(|s|s.trim().is_empty()){
            let (code,message)=if modification==ModificationPermission::Prohibited{
                ("merge.license.prohibited",format!("Part {index} ({label}) explicitly prohibits modification"))
            }else{
                ("merge.license.unknown",format!("Part {index} ({label}) has unknown license or modification permission; confirm modification and redistribution rights"))
            };
            diagnostic(report,index,code,Impact::None,message);
        }
        let bytes=copyright.as_ref().map_or(0,String::len)+license.as_ref().map_or(0,String::len)+license_url.as_ref().map_or(0,String::len)+reserved_names.iter().map(String::len).sum::<usize>()+label.len()+provenance.len();
        budget.charge(bytes.saturating_mul(4).saturating_add(512))?;
        result.push(LicenseReport{part_index:index,label,copyright,license,license_url,modification,provenance,reserved_names});
    }
    Ok(result)
}
pub(super) fn write_names(font:&mut Font,options:&MergeOptions,licenses:&[LicenseReport]){
    let header=format!("Merged by fontpixel; parts: {}",licenses.iter().map(|l|l.label.as_str()).collect::<Vec<_>>().join(", "));
    let combine_field=|field:fn(&LicenseReport)->&Option<String>|{
        let mut text=header.clone();
        for item in licenses{text.push('\n');text.push_str(&format!("[{}] {}",item.label,field(item).as_deref().unwrap_or("Unknown")));}
        text
    };
    font.names.family=Some(options.family.clone());font.names.full=Some(options.family.clone());font.names.style=Some("Regular".into());
    let ps:String=options.family.chars().filter(|c|c.is_ascii_alphanumeric()||*c=='-'||*c=='_').collect();
    font.names.postscript=Some(if ps.is_empty(){"FontpixelMerged".into()}else{ps});
    font.names.copyright=Some(combine_field(|l|&l.copyright));font.names.license=Some(combine_field(|l|&l.license));
    if licenses.iter().any(|l|l.license_url.is_some()){font.names.license_url=Some(combine_field(|l|&l.license_url));}
    font.properties.set("COPYRIGHT",font.names.copyright.clone().unwrap_or_default());
    font.properties.set("LICENSE",font.names.license.clone().unwrap_or_default());
    if let Some(url)=&font.names.license_url{font.properties.set("LICENSE_URL",url.clone());}
}
