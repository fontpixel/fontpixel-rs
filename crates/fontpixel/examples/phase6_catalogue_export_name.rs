//! Explicit catalogue migration companion: phase6_catalogue_export_name PLAN.json.
//! Build/test with --all-features. This example is autodiscovered; no Cargo entry
//! or public library policy is added. Invoke it inside the measured workflow.
#[cfg(all(feature = "bdf", feature = "pcf", feature = "serde"))]
#[path = "../../../benches/support/catalogue_export_name.rs"]
mod rename;

#[cfg(all(feature = "bdf", feature = "pcf", feature = "serde"))]
mod enabled {
    use super::rename::{self, Budget};
    use fontpixel::formats::{Confidence, bdf, pcf};
    use fontpixel::model::{GlyphBitmap, PropertyValue};
    use fontpixel::{Error, Font, Input, Report, ReportEntry, ResourceLimits};
    use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
    use serde::ser::{SerializeSeq, SerializeStruct};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::fmt;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::mem::{align_of, size_of};
    use std::path::Path;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

    struct Job {
        input: String,
        output: String,
        export_name: String,
        size: i64,
        weight: String,
        width: String,
        spacing: String,
        script_subset: Option<String>,
        weight_capitalized: String,
    }
    struct Plan {
        jobs: Vec<Job>,
    }

    // Numeric type errors must not format an arbitrarily long JSON string into
    // an unbudgeted error message. Accept only signed/unsigned integer visits.
    struct IntegerSeed;
    impl<'de> DeserializeSeed<'de> for IntegerSeed {
        type Value = i64;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<i64, D::Error> {
            struct IntegerVisitor;
            impl Visitor<'_> for IntegerVisitor {
                type Value = i64;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a signed 64-bit integer")
                }
                fn visit_i64<E: de::Error>(self, n: i64) -> std::result::Result<i64, E> {
                    Ok(n)
                }
                fn visit_u64<E: de::Error>(self, n: u64) -> std::result::Result<i64, E> {
                    i64::try_from(n).map_err(|_| E::custom("catalogue integer is out of range"))
                }
                fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<i64, E> {
                    Err(E::custom("catalogue integer cannot be a string"))
                }
            }
            d.deserialize_any(IntegerVisitor)
        }
    }

    // Do not use derive(Deserialize) for an unbounded Vec<Job>. Every retained
    // string and vector allocation passes through the same live-storage ledger.
    struct TextSeed<'a>(&'a mut Budget);
    impl<'de> DeserializeSeed<'de> for TextSeed<'_> {
        type Value = String;
        fn deserialize<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<String, D::Error> {
            struct TextVisitor<'a>(&'a mut Budget);
            impl Visitor<'_> for TextVisitor<'_> {
                type Value = String;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a bounded string")
                }
                fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<String, E> {
                    self.0.text(value).map_err(E::custom)
                }
            }
            deserializer.deserialize_str(TextVisitor(self.0))
        }
    }
    struct OptionalTextSeed<'a>(&'a mut Budget);
    impl<'de> DeserializeSeed<'de> for OptionalTextSeed<'_> {
        type Value = Option<String>;
        fn deserialize<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            struct OptionalVisitor<'a>(&'a mut Budget);
            impl<'de> Visitor<'de> for OptionalVisitor<'_> {
                type Value = Option<String>;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a string or null")
                }
                fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                    Ok(None)
                }
                fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                    Ok(None)
                }
                fn visit_some<D: Deserializer<'de>>(
                    self,
                    d: D,
                ) -> std::result::Result<Self::Value, D::Error> {
                    TextSeed(self.0).deserialize(d).map(Some)
                }
            }
            deserializer.deserialize_option(OptionalVisitor(self.0))
        }
    }
    #[derive(Clone, Copy)]
    enum Field {
        Version,
        Jobs,
        Input,
        Output,
        ExportName,
        Size,
        Weight,
        Width,
        Spacing,
        ScriptSubset,
        WeightCapitalized,
    }
    impl<'de> Deserialize<'de> for Field {
        fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            struct FieldVisitor;
            impl Visitor<'_> for FieldVisitor {
                type Value = Field;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a known version-1 field")
                }
                fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Field, E> {
                    Ok(match value {
                        "version" => Field::Version,
                        "jobs" => Field::Jobs,
                        "input" => Field::Input,
                        "output" => Field::Output,
                        "export_name" => Field::ExportName,
                        "size" => Field::Size,
                        "weight" => Field::Weight,
                        "width" => Field::Width,
                        "spacing" => Field::Spacing,
                        "script_subset" => Field::ScriptSubset,
                        "weight_capitalized" => Field::WeightCapitalized,
                        // Avoid copying an attacker-sized unknown key into an error.
                        _ => return Err(E::custom("unknown catalogue plan field")),
                    })
                }
            }
            d.deserialize_identifier(FieldVisitor)
        }
    }
    struct JobSeed<'a> {
        budget: &'a mut Budget,
        allowed: bool,
    }
    impl<'de> DeserializeSeed<'de> for JobSeed<'_> {
        type Value = Job;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Job, D::Error> {
            if !self.allowed {
                return Err(de::Error::custom(
                    "catalogue job count exceeds max_resources",
                ));
            }
            struct JobVisitor<'a>(&'a mut Budget);
            impl<'de> Visitor<'de> for JobVisitor<'_> {
                type Value = Job;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a catalogue rename job")
                }
                fn visit_map<M: MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> std::result::Result<Job, M::Error> {
                    let (mut input, mut output, mut export_name, mut size) =
                        (None, None, None, None);
                    let (
                        mut weight,
                        mut width,
                        mut spacing,
                        mut script_subset,
                        mut weight_capitalized,
                    ) = (None, None, None, None, None);
                    macro_rules! text {
                        ($slot:ident) => {{
                            if $slot.is_some() {
                                return Err(de::Error::custom("duplicate catalogue job field"));
                            }
                            $slot = Some(map.next_value_seed(TextSeed(&mut *self.0))?);
                        }};
                    }
                    while let Some(field) = map.next_key::<Field>()? {
                        match field {
                            Field::Input => text!(input),
                            Field::Output => text!(output),
                            Field::ExportName => text!(export_name),
                            Field::Weight => text!(weight),
                            Field::Width => text!(width),
                            Field::Spacing => text!(spacing),
                            Field::WeightCapitalized => text!(weight_capitalized),
                            Field::Size => {
                                if size.is_some() {
                                    return Err(de::Error::custom("duplicate size"));
                                }
                                size = Some(map.next_value_seed(IntegerSeed)?);
                            }
                            Field::ScriptSubset => {
                                if script_subset.is_some() {
                                    return Err(de::Error::custom("duplicate script_subset"));
                                }
                                script_subset =
                                    Some(map.next_value_seed(OptionalTextSeed(&mut *self.0))?);
                            }
                            _ => return Err(de::Error::custom("unexpected catalogue job field")),
                        }
                    }
                    macro_rules! required {
                        ($field:ident) => {
                            $field.ok_or_else(|| de::Error::missing_field(stringify!($field)))?
                        };
                    }
                    Ok(Job {
                        input: required!(input),
                        output: required!(output),
                        export_name: required!(export_name),
                        size: required!(size),
                        weight: required!(weight),
                        width: required!(width),
                        spacing: required!(spacing),
                        script_subset: script_subset.unwrap_or(None),
                        weight_capitalized: required!(weight_capitalized),
                    })
                }
            }
            d.deserialize_map(JobVisitor(self.budget))
        }
    }
    struct JobsSeed<'a> {
        budget: &'a mut Budget,
        maximum_jobs: usize,
    }
    impl<'de> DeserializeSeed<'de> for JobsSeed<'_> {
        type Value = Vec<Job>;
        fn deserialize<D: Deserializer<'de>>(
            self,
            d: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            struct JobsVisitor<'a>(JobsSeed<'a>);
            impl<'de> Visitor<'de> for JobsVisitor<'_> {
                type Value = Vec<Job>;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("bounded catalogue jobs")
                }
                fn visit_seq<S: SeqAccess<'de>>(
                    self,
                    mut seq: S,
                ) -> std::result::Result<Self::Value, S::Error> {
                    let mut jobs = Vec::new();
                    // Ignore untrusted size_hint; reject an excess item before
                    // deserializing any of its fields or reserving its storage.
                    while let Some(job) = seq.next_element_seed(JobSeed {
                        budget: &mut *self.0.budget,
                        allowed: jobs.len() < self.0.maximum_jobs,
                    })? {
                        if jobs.len() == jobs.capacity() {
                            let old_capacity = jobs.capacity();
                            let count = jobs
                                .len()
                                .checked_add(1)
                                .ok_or_else(|| de::Error::custom("job count overflow"))?;
                            // Reserve the full new allocation while the old one
                            // still exists, then release the old header storage.
                            self.0
                                .budget
                                .array::<Job>(count)
                                .map_err(de::Error::custom)?;
                            jobs.try_reserve_exact(1)
                                .map_err(|_| de::Error::custom("job allocation failed"))?;
                            self.0
                                .budget
                                .array::<Job>(jobs.capacity().saturating_sub(count))
                                .map_err(de::Error::custom)?;
                            self.0.budget.used -= old_capacity * size_of::<Job>();
                        }
                        jobs.push(job);
                    }
                    Ok(jobs)
                }
            }
            d.deserialize_seq(JobsVisitor(self))
        }
    }
    struct PlanSeed<'a> {
        budget: &'a mut Budget,
        maximum_jobs: usize,
    }
    impl<'de> DeserializeSeed<'de> for PlanSeed<'_> {
        type Value = Plan;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Plan, D::Error> {
            struct PlanVisitor<'a>(PlanSeed<'a>);
            impl<'de> Visitor<'de> for PlanVisitor<'_> {
                type Value = Plan;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a version-1 catalogue plan")
                }
                fn visit_map<M: MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> std::result::Result<Plan, M::Error> {
                    let mut version = None;
                    let mut jobs = None;
                    while let Some(field) = map.next_key::<Field>()? {
                        match field {
                            Field::Version if version.is_none() => {
                                version = Some(map.next_value_seed(IntegerSeed)?)
                            }
                            Field::Jobs if jobs.is_none() => {
                                jobs = Some(map.next_value_seed(JobsSeed {
                                    budget: &mut *self.0.budget,
                                    maximum_jobs: self.0.maximum_jobs,
                                })?)
                            }
                            _ => {
                                return Err(de::Error::custom(
                                    "duplicate or unexpected catalogue plan field",
                                ));
                            }
                        }
                    }
                    if version != Some(1) {
                        return Err(de::Error::custom("catalogue plan version must be 1"));
                    }
                    Ok(Plan {
                        jobs: jobs.ok_or_else(|| de::Error::missing_field("jobs"))?,
                    })
                }
            }
            d.deserialize_map(PlanVisitor(self))
        }
    }

    fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("catalogue input must be a regular file".into());
        }
        let length = usize::try_from(metadata.len()).map_err(|_| rename::limit(maximum))?;
        let mut budget = Budget { used: 0, maximum };
        let mut bytes = budget.vector(length)?;
        bytes.resize(length, 0);
        file.read_exact(&mut bytes)?;
        // A growing file must not trigger read_to_end's geometric allocation.
        let mut extra = [0; 1];
        if file.read(&mut extra)? != 0 {
            return Err("catalogue input changed while reading".into());
        }
        Ok(bytes)
    }
    fn parse_plan(raw: &[u8], capacity: usize, limits: &ResourceLimits) -> Result<Plan> {
        let mut budget = Budget {
            used: capacity,
            maximum: limits.max_total_bytes,
        };
        // serde_json's slice deserializer borrows ordinary strings. Its reused
        // escape/number scratch Vec grows geometrically; unescaping UTF-8 never
        // expands past wire length. Reserve both scratch capacities, including
        // old+new during growth, separately from retained strings charged above.
        budget.charge(raw.len())?;
        budget.charge(raw.len())?;
        budget.charge(raw.len())?;
        budget.charge(1024)?; // Bounded schema/error text; never embeds input keys.
        let mut decoder = serde_json::Deserializer::from_slice(raw);
        let plan = PlanSeed {
            budget: &mut budget,
            maximum_jobs: limits.max_resources,
        }
        .deserialize(&mut decoder)?;
        decoder.end()?;
        Ok(plan)
    }
    fn plan_bytes(plan: &Plan, maximum: usize) -> fontpixel::Result<usize> {
        let mut budget = Budget {
            used: size_of::<Plan>(),
            maximum,
        };
        budget.array::<Job>(plan.jobs.capacity())?;
        for job in &plan.jobs {
            for s in [
                &job.input,
                &job.output,
                &job.export_name,
                &job.weight,
                &job.width,
                &job.spacing,
                &job.weight_capitalized,
            ] {
                budget.charge(s.capacity())?;
            }
            budget.charge(job.script_subset.as_ref().map_or(0, String::capacity))?;
        }
        Ok(budget.used)
    }
    fn report_bytes(report: &Report, maximum: usize) -> fontpixel::Result<usize> {
        let mut budget = Budget {
            used: size_of::<Report>(),
            maximum,
        };
        budget.array::<ReportEntry>(report.entries.capacity())?;
        for entry in &report.entries {
            budget.charge(entry.code.capacity())?;
            budget.charge(entry.message.capacity())?;
            budget.charge(entry.location.asset.as_ref().map_or(0, String::capacity))?;
        }
        Ok(budget.used)
    }

    // This adapter calls the existing BDF writer, which exposes only an output
    // length cap. Reserve its working structures before that call. These are
    // source-derived bounds for bdf/writer.rs and x11.rs, not a model multiplier.
    // A change to those allocation paths requires reviewing this reservation.
    fn writer_reservation(font: &Font, maximum: usize) -> fontpixel::Result<usize> {
        let mut budget = Budget { used: 0, maximum };
        let metadata = rename::property_bytes(&font.headers, maximum)?
            .checked_add(rename::property_bytes(&font.properties, maximum)?)
            .ok_or_else(|| rename::limit(maximum))?;
        // h/p from reconcile, target's h/p clone, and original()'s restored
        // metadata can coexist. Reallocation of a metadata vector can coexist
        // with its old buffer, hence one further pair at peak.
        for _ in 0..4 {
            budget.charge(metadata)?;
        }
        fn views(
            value: &PropertyValue,
            budget: &mut Budget,
            depth: usize,
        ) -> fontpixel::Result<()> {
            match value {
                PropertyValue::Text(s) | PropertyValue::Decimal(s) => {
                    // A projected string, a full XLFD mirror, and decomposed
                    // field strings; the latter need their own String headers.
                    budget.charge(s.len())?;
                    budget.charge(s.len())?;
                    budget.charge(s.len())?;
                    let fields = s.bytes().filter(|&b| b == b'-').count().saturating_add(1);
                    // Iterator collect may double its field vector and retain
                    // both buffers during growth, with minimum capacity four.
                    budget.array::<String>(
                        fields
                            .checked_mul(3)
                            .and_then(|n| n.checked_add(4))
                            .ok_or_else(|| rename::limit(budget.maximum))?,
                    )?;
                }
                PropertyValue::Integer(_) | PropertyValue::Unsigned(_) => budget.charge(22)?,
                PropertyValue::List(v) => {
                    if depth >= 128 {
                        return Err(rename::limit(budget.maximum));
                    }
                    for value in v {
                        views(value, budget, depth + 1)?;
                    }
                }
                _ => (),
            }
            Ok(())
        }
        // target and original each own projected name/charset views.
        for _ in 0..2 {
            for p in font.headers.iter().chain(font.properties.iter()) {
                views(&p.value, &mut budget, 0)?;
            }
        }
        // Reconciliation may copy normalized name/charset strings independently
        // of the raw fields. Include every such current view and its field Vec.
        for s in [
            &font.names.family,
            &font.names.style,
            &font.names.full,
            &font.names.postscript,
            &font.names.version,
            &font.names.copyright,
            &font.names.license,
            &font.names.license_url,
            &font.names.foundry,
            &font.names.xlfd,
            &font.charset.name,
            &font.charset.registry,
            &font.charset.encoding,
        ] {
            if let Some(s) = s {
                budget.charge(s.capacity())?;
            }
        }
        budget.array::<String>(font.names.xlfd_fields.capacity())?;
        for s in &font.names.xlfd_fields {
            budget.charge(s.capacity())?;
        }
        // `set` can append fixed X11 fields. Reserve both old/new vector storage
        // for all 32 supported global fields, without trusting declared counts.
        budget.array::<fontpixel::model::Property>(64)?;
        budget.charge(2048)?; // Fixed key strings and numeric/global line buffers.

        fn tree<K, V>(budget: &mut Budget, entries: usize) -> fontpixel::Result<()> {
            // Same conservative per-entry node layout used by model accounting:
            // 16 key/value/edge slots + 128 header bytes, plus an empty root.
            let node = 16 * (size_of::<K>() + size_of::<V>() + size_of::<usize>()) + 128;
            budget.charge(
                entries
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(node))
                    .ok_or_else(|| rename::limit(budget.maximum))?,
            )
        }
        tree::<&str, usize>(&mut budget, font.headers.len())?;
        tree::<String, usize>(&mut budget, font.properties.len())?;
        budget.array::<&fontpixel::model::Property>(
            font.properties.len().saturating_mul(6).saturating_add(8),
        )?;
        for p in font.properties.iter() {
            // Count Unicode lowercase expansion without allocating or guessing
            // a byte multiplier. Reserve the initial and growing key buffers.
            let lower_bytes = p
                .name
                .chars()
                .flat_map(char::to_lowercase)
                .try_fold(0usize, |n, c| n.checked_add(c.len_utf8()))
                .ok_or_else(|| rename::limit(maximum))?;
            budget.charge(p.name.len())?;
            budget.charge(
                lower_bytes
                    .checked_mul(3)
                    .ok_or_else(|| rename::limit(maximum))?,
            )?;
        }

        let mut codes = 0usize;
        let mut diagnostics = 8usize; // All global/charset/name diagnostics.
        let mut dynamic_names = 0usize;
        let mut largest_raster = 0usize;
        let mut largest_name = 0usize;
        fn count(n: &mut usize, add: usize, maximum: usize) -> fontpixel::Result<()> {
            *n = n.checked_add(add).ok_or_else(|| rename::limit(maximum))?;
            Ok(())
        }
        for (_, glyph) in font.glyphs() {
            count(&mut codes, glyph.codepoints.len(), maximum)?;
            count(
                &mut diagnostics,
                usize::from(!glyph.unicode_sequences.is_empty())
                    + usize::from(glyph.codepoints.len() > 1),
                maximum,
            )?;
            for present in [
                glyph.metrics.abc.is_some(),
                glyph.metrics.left_bearing.is_some(),
                glyph.metrics.right_bearing.is_some(),
                glyph.metrics.image_offset.is_some(),
            ] {
                count(&mut diagnostics, usize::from(present), maximum)?;
            }
            for p in glyph.properties.iter() {
                if p.raw.is_some()
                    || !matches!(
                        p.name.as_str(),
                        "bdf.bitmap.rows" | "bdf.bitmap.image" | "bdf.altcode"
                    )
                {
                    count(&mut diagnostics, 1, maximum)?;
                    count(&mut dynamic_names, p.name.len(), maximum)?;
                }
            }
            let name = glyph.name.as_deref().unwrap_or("");
            largest_name = largest_name.max(name.len());
            if name.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{b}' | '\u{c}'))
                != name
            {
                count(&mut diagnostics, glyph.codepoints.len().max(1), maximum)?;
            }
            largest_raster = largest_raster.max(glyph.bitmap.as_bytes().len());
            // Read-only BDF/PCF inputs are Mono. Reject unexpected expansion
            // rather than silently introducing a Gray8 threshold.
            if !matches!(glyph.bitmap, GlyphBitmap::Mono(_)) {
                return Err(Error::InvalidInput(
                    "catalogue BDF/PCF input must be monochrome".into(),
                ));
            }
        }
        // Duplicate, unmappable and collision paths are mutually exclusive per
        // codepoint. Source properties and localized keys each have diagnostics.
        count(&mut diagnostics, codes, maximum)?;
        count(&mut diagnostics, font.headers.len(), maximum)?;
        count(
            &mut diagnostics,
            font.properties
                .len()
                .checked_mul(2)
                .ok_or_else(|| rename::limit(maximum))?,
            maximum,
        )?;
        count(&mut diagnostics, font.names.localized.len(), maximum)?;
        for p in font
            .headers
            .iter()
            .chain(font.properties.iter())
            .chain(font.names.localized.iter())
        {
            count(
                &mut dynamic_names,
                p.name
                    .len()
                    .checked_mul(2)
                    .ok_or_else(|| rename::limit(maximum))?,
                maximum,
            )?;
        }
        // Report Vec growth (including old/new buffers), per-entry code <=64
        // bytes and fixed text <=192 bytes in the current BDF writer; formatting
        // capacity and escaped debug names are reserved independently.
        budget.array::<ReportEntry>(
            diagnostics
                .checked_mul(3)
                .and_then(|n| n.checked_add(4))
                .ok_or_else(|| rename::limit(maximum))?,
        )?;
        budget.charge(
            diagnostics
                .checked_mul(64 + 2 * 192)
                .ok_or_else(|| rename::limit(maximum))?,
        )?;
        budget.charge(
            dynamic_names
                .checked_mul(12)
                .ok_or_else(|| rename::limit(maximum))?,
        )?;
        tree::<u32, usize>(&mut budget, codes)?;
        let record_size = size_of::<fontpixel::model::GlyphId>()
            + size_of::<&fontpixel::Glyph>()
            + size_of::<Option<u32>>()
            + 3 * align_of::<usize>();
        let record_count = codes
            .checked_add(font.len())
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| rename::limit(maximum))?;
        budget.charge(
            record_count
                .checked_mul(record_size)
                .ok_or_else(|| rename::limit(maximum))?,
        )?;
        // A cloned mono bitmap, hex-row bytes (at most twice raster length),
        // source/format name buffers. Maxima suffice because glyphs serialize
        // sequentially. The empty-raster case gets the fixed line allowance.
        budget.charge(largest_raster)?;
        budget.charge(
            largest_raster
                .checked_mul(2)
                .ok_or_else(|| rename::limit(maximum))?,
        )?;
        budget.charge(
            largest_name
                .checked_mul(3)
                .ok_or_else(|| rename::limit(maximum))?,
        )?;
        Ok(budget.used)
    }

    struct Reports<'a>([&'a Report; 3]);
    impl Serialize for Reports<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
            struct Entries<'a>(&'a Reports<'a>);
            impl Serialize for Entries<'_> {
                fn serialize<S: Serializer>(
                    &self,
                    serializer: S,
                ) -> std::result::Result<S::Ok, S::Error> {
                    let count = self
                        .0
                        .0
                        .iter()
                        .try_fold(0usize, |n, r| n.checked_add(r.entries.len()))
                        .ok_or_else(|| {
                            serde::ser::Error::custom("catalogue report count overflow")
                        })?;
                    let mut seq = serializer.serialize_seq(Some(count))?;
                    for report in self.0.0 {
                        for entry in &report.entries {
                            seq.serialize_element(entry)?;
                        }
                    }
                    seq.end()
                }
            }
            let mut state = serializer.serialize_struct("Report", 1)?;
            state.serialize_field("entries", &Entries(self))?;
            state.end()
        }
    }
    #[derive(Serialize)]
    struct ResultLine<'a> {
        index: usize,
        input: &'a str,
        output: &'a str,
        output_bytes: usize,
        report: Reports<'a>,
    }
    struct BoundedWriter<'a, W> {
        inner: &'a mut W,
        used: usize,
        maximum: usize,
    }
    impl<W: Write> Write for BoundedWriter<'_, W> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.maximum.saturating_sub(self.used) {
                return Err(std::io::Error::other(
                    "catalogue JSONL report budget exceeded",
                ));
            }
            let n = self.inner.write(bytes)?;
            self.used += n;
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
    struct Rollback<'a> {
        files: Vec<&'a Path>,
        committed: bool,
    }
    impl Rollback<'_> {
        fn cleanup(&mut self) -> std::io::Result<()> {
            let mut first = None;
            while let Some(path) = self.files.pop() {
                if let Err(error) = std::fs::remove_file(path) {
                    if first.is_none() {
                        first = Some(error);
                    }
                }
            }
            first.map_or(Ok(()), Err)
        }
    }
    impl Drop for Rollback<'_> {
        fn drop(&mut self) {
            if !self.committed {
                let _ = self.cleanup();
            }
        }
    }

    fn run_jobs<W: Write>(plan: &Plan, limits: &ResourceLimits, stdout: &mut W) -> Result<()> {
        let mut retained = Budget {
            used: plan_bytes(plan, limits.max_total_bytes)?,
            maximum: limits.max_total_bytes,
        };
        if plan.jobs.len() > limits.max_resources {
            return Err(rename::limit(limits.max_resources).into());
        }
        let mut rollback = Rollback {
            files: retained.vector(plan.jobs.len())?,
            committed: false,
        };
        let plan_retained = retained.used;
        let mut stream = BoundedWriter {
            inner: stdout,
            used: 0,
            maximum: limits.max_total_bytes,
        };
        let result = (|| -> Result<()> {
            let mut output_total = 0usize;
            for (index, job) in plan.jobs.iter().enumerate() {
                if job.export_name.is_empty()
                    || Path::new(&job.output).extension().and_then(|s| s.to_str()) != Some("bdf")
                {
                    return Err(
                        "catalogue jobs require explicit export_name and a new .bdf output".into(),
                    );
                }
                let mut local = limits.clone();
                local.max_total_bytes = limits.max_total_bytes - plan_retained;
                let (mut font, read_report) = {
                    let input = read_bounded(
                        Path::new(&job.input),
                        local.max_input_bytes.min(local.max_total_bytes),
                    )?;
                    let source = Input::new(&input);
                    let mut context_limits = local.clone();
                    context_limits.max_total_bytes -= input.capacity() - input.len();
                    let context = fontpixel::resources::ReadContext::new(&source, &context_limits)?;
                    let mut parser_limits = local.clone();
                    parser_limits.max_total_bytes = context_limits
                        .max_total_bytes
                        .checked_sub(context.retained_input_bytes())
                        .ok_or_else(|| rename::limit(local.max_total_bytes))?;
                    let bytes = context.bytes();
                    if bdf::detect(bytes) != Confidence::NoMatch {
                        bdf::read(bytes, &parser_limits)?
                    } else if pcf::detect(bytes) != Confidence::NoMatch {
                        pcf::read(bytes, &parser_limits)?
                    } else {
                        return Err("catalogue export-name accepts BDF/PCF input only".into());
                    }
                }; // Input wire/decompression storage is gone before edit staging.
                if read_report.has_errors() {
                    return Err(Error::ReportedErrors(read_report).into());
                }
                let read_bytes = report_bytes(&read_report, local.max_total_bytes)?;
                local.max_total_bytes = local
                    .max_total_bytes
                    .checked_sub(read_bytes)
                    .ok_or_else(|| rename::limit(limits.max_total_bytes))?;
                let policy_report = rename::apply(
                    &mut font,
                    &rename::ExportName {
                        name: &job.export_name,
                        size: job.size,
                        weight: &job.weight,
                        weight_capitalized: &job.weight_capitalized,
                        width: &job.width,
                        spacing: &job.spacing,
                        script_subset: job.script_subset.as_deref(),
                    },
                    &local,
                )?;
                let mut live = Budget {
                    used: plan_retained,
                    maximum: limits.max_total_bytes,
                };
                live.charge(read_bytes)?;
                live.charge(report_bytes(&policy_report, live.maximum)?)?;
                live.charge(font.retained_usage(live.maximum)?.bytes)?;
                live.charge(writer_reservation(&font, live.maximum)?)?;
                // String growth can retain old+new buffers during reallocation;
                // three output lengths cover that peak. This restricts only
                // this explicit companion, never native global format defaults.
                let output_limit = ((live.maximum - live.used) / 3)
                    .min(limits.max_total_bytes - output_total)
                    .min(bdf::BdfWriteOptions::default().max_output_bytes);
                let (bytes, write_report) = bdf::write(
                    &font,
                    &bdf::BdfWriteOptions {
                        max_output_bytes: output_limit,
                        ..Default::default()
                    },
                )?;
                if write_report.has_errors() {
                    return Err(Error::ReportedErrors(write_report).into());
                }
                // Recount actual retained results before any filesystem effect;
                // the writer reservation above is scratch, not retained twice.
                let mut completed = Budget {
                    used: plan_retained,
                    maximum: limits.max_total_bytes,
                };
                completed.charge(font.retained_usage(completed.maximum)?.bytes)?;
                completed.charge(read_bytes)?;
                completed.charge(report_bytes(&policy_report, completed.maximum)?)?;
                completed.charge(report_bytes(&write_report, completed.maximum)?)?;
                completed.charge(bytes.capacity())?;
                output_total = output_total
                    .checked_add(bytes.len())
                    .filter(|&n| n <= limits.max_total_bytes)
                    .ok_or_else(|| rename::limit(limits.max_total_bytes))?;
                let path = Path::new(&job.output);
                let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
                rollback.files.push(path); // Capacity was reserved before creating any file.
                output.write_all(&bytes)?;
                output.flush()?;
                drop(output);
                serde_json::to_writer(
                    &mut stream,
                    &ResultLine {
                        index,
                        input: &job.input,
                        output: &job.output,
                        output_bytes: bytes.len(),
                        report: Reports([&read_report, &policy_report, &write_report]),
                    },
                )?;
                stream.write_all(b"\n")?;
            }
            stream.flush()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                rollback.committed = true;
                Ok(())
            }
            Err(error) => {
                // Do not hide a cleanup failure. The caller owns an exclusive
                // temporary directory; pre-existing outputs are never tracked.
                if let Err(cleanup) = rollback.cleanup() {
                    return Err(format!("{error}; catalogue cleanup failed: {cleanup}").into());
                }
                Err(error)
            }
        }
    }

    pub fn run() -> Result<()> {
        let mut args = std::env::args_os();
        let _program = args.next();
        let path = args
            .next()
            .ok_or("phase6_catalogue_export_name PLAN.json")?;
        if args.next().is_some() {
            return Err("phase6_catalogue_export_name PLAN.json".into());
        }
        let limits = ResourceLimits::default();
        // Wire + three scratch capacities must fit before opening the decoder;
        // retained jobs are separately charged by the seed visitors.
        let raw = read_bounded(
            Path::new(&path),
            limits
                .max_input_bytes
                .min(limits.max_table_bytes)
                .min(limits.max_total_bytes.saturating_sub(1024) / 4),
        )?;
        let plan = parse_plan(&raw, raw.capacity(), &limits)?;
        drop(raw);
        run_jobs(&plan, &limits, &mut std::io::stdout().lock())
    }

    // Source-only recovery tests. These have not been run in the reconstruction
    // environment; run `cargo test -p fontpixel --all-features --example
    // phase6_catalogue_export_name` when the coordinated native slot is released.
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};
        const SOURCE: &[u8] = b"STARTFONT 2.1\nFONT Old\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 1 0 0\nCOMMENT keep this notice\nSTARTPROPERTIES 4\nFAMILY_NAME \"Old\"\nCOPYRIGHT \"Copyright original\"\nLICENSE \"OFL-1.1\"\nFACE_NAME \"Old Face\"\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 8 1 0 0\nBITMAP\n81\nENDCHAR\nENDFONT\n";
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Directory(PathBuf);
        impl Directory {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "fontpixel-catalogue-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn path(&self, name: &str) -> String {
                self.0.join(name).to_str().unwrap().into()
            }
        }
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        fn job(dir: &Directory, output: &str) -> Job {
            Job {
                input: dir.path("original.bdf"),
                output: dir.path(output),
                export_name: "New Family".into(),
                size: 8,
                weight: "regular".into(),
                weight_capitalized: "Regular".into(),
                width: "normal".into(),
                spacing: "mono".into(),
                script_subset: None,
            }
        }
        fn json_job() -> &'static str {
            r#"{"input":"original.bdf","output":"renamed.bdf","export_name":"New Family","size":8,"weight":"regular","weight_capitalized":"Regular","width":"normal","spacing":"mono","script_subset":null}"#
        }
        #[test]
        fn schema_rejects_excess_jobs_duplicates_unknown_keys_and_trailing_data() {
            let good = format!("{{\"version\":1,\"jobs\":[{}]}}", json_job());
            let limits = ResourceLimits::default();
            assert_eq!(
                parse_plan(good.as_bytes(), good.len(), &limits)
                    .unwrap()
                    .jobs
                    .len(),
                1
            );
            for bad in [
                good.replace("\"version\":1", "\"version\":1,\"version\":1"),
                good.replace("\"size\":8", "\"size\":8,\"size\":9"),
                good.replace("\"size\":8", "\"size\":\"not a number\""),
                good.replace("\"size\":8", "\"size\":8,\"unknown\":0"),
                format!("{good} null"),
            ] {
                assert!(parse_plan(bad.as_bytes(), bad.len(), &limits).is_err());
            }
            let pair = format!("{{\"version\":1,\"jobs\":[{},{}]}}", json_job(), json_job());
            let limited = ResourceLimits {
                max_resources: 1,
                ..limits.clone()
            };
            assert!(parse_plan(pair.as_bytes(), pair.len(), &limited).is_err());
            let limited = ResourceLimits {
                max_total_bytes: good.len(),
                ..limits
            };
            assert!(parse_plan(good.as_bytes(), good.len(), &limited).is_err());
        }
        #[test]
        fn successful_output_keeps_source_bytes_notices_and_complete_report_order() {
            let dir = Directory::new();
            std::fs::write(dir.path("original.bdf"), SOURCE).unwrap();
            let plan = Plan {
                jobs: vec![job(&dir, "renamed.bdf")],
            };
            let mut stdout = Vec::new();
            run_jobs(&plan, &Default::default(), &mut stdout).unwrap();
            assert_eq!(std::fs::read(dir.path("original.bdf")).unwrap(), SOURCE);
            let result: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            let entries = result["report"]["entries"].as_array().unwrap();
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry["code"] == "catalogue.export_name")
                    .count(),
                1
            );
            let output = std::fs::read_to_string(dir.path("renamed.bdf")).unwrap();
            assert!(output.contains("FONT New-Family-8px-mono\n"));
            assert!(output.contains("FAMILY_NAME \"New Family\""));
            assert!(output.contains("FACE_NAME \"New Family Regular\""));
            assert!(output.contains("COPYRIGHT \"Copyright original\""));
            assert!(output.contains("LICENSE \"OFL-1.1\""));
            assert!(output.contains("COMMENT \"keep this notice\""));
            let original = bdf::read(SOURCE, &Default::default()).unwrap().0;
            let renamed = bdf::read(output.as_bytes(), &Default::default()).unwrap().0;
            assert_eq!(
                original.glyphs().next().unwrap().1.bitmap,
                renamed.glyphs().next().unwrap().1.bitmap
            );
            assert_eq!(
                original.glyphs().next().unwrap().1.metrics,
                renamed.glyphs().next().unwrap().1.metrics
            );
            assert_eq!(
                original.glyphs().next().unwrap().1.codepoints,
                renamed.glyphs().next().unwrap().1.codepoints
            );
        }
        #[test]
        fn late_destination_failure_removes_only_files_created_by_this_run() {
            let dir = Directory::new();
            std::fs::write(dir.path("original.bdf"), SOURCE).unwrap();
            std::fs::write(dir.path("existing.bdf"), b"do not replace").unwrap();
            let plan = Plan {
                jobs: vec![job(&dir, "first.bdf"), job(&dir, "existing.bdf")],
            };
            assert!(run_jobs(&plan, &Default::default(), &mut Vec::new()).is_err());
            assert!(!Path::new(&dir.path("first.bdf")).exists());
            assert_eq!(
                std::fs::read(dir.path("existing.bdf")).unwrap(),
                b"do not replace"
            );
            assert_eq!(std::fs::read(dir.path("original.bdf")).unwrap(), SOURCE);
            let plan = Plan {
                jobs: vec![job(&dir, "original.bdf")],
            };
            assert!(run_jobs(&plan, &Default::default(), &mut Vec::new()).is_err());
            assert_eq!(std::fs::read(dir.path("original.bdf")).unwrap(), SOURCE);
        }
        #[test]
        fn late_metadata_budget_failure_cleans_outputs_and_preserves_original() {
            let dir = Directory::new();
            std::fs::write(dir.path("original.bdf"), SOURCE).unwrap();
            let mut second = job(&dir, "second.bdf");
            second.export_name = "x".repeat(2049);
            let plan = Plan {
                jobs: vec![job(&dir, "first.bdf"), second],
            };
            let limits = ResourceLimits {
                max_table_bytes: 2048,
                ..Default::default()
            };
            let mut stdout = Vec::new();
            assert!(run_jobs(&plan, &limits, &mut stdout).is_err());
            assert!(
                !stdout.is_empty(),
                "first job must finish before the intended late failure"
            );
            assert!(!Path::new(&dir.path("first.bdf")).exists());
            assert!(!Path::new(&dir.path("second.bdf")).exists());
            assert_eq!(std::fs::read(dir.path("original.bdf")).unwrap(), SOURCE);
        }
        #[test]
        fn report_sink_failure_rolls_back_and_non_bdf_pcf_sources_are_rejected() {
            struct Fails;
            impl Write for Fails {
                fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                    Err(std::io::Error::other("injected report failure"))
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let dir = Directory::new();
            std::fs::write(dir.path("original.bdf"), SOURCE).unwrap();
            let plan = Plan {
                jobs: vec![job(&dir, "renamed.bdf")],
            };
            assert!(run_jobs(&plan, &Default::default(), &mut Fails).is_err());
            assert!(!Path::new(&dir.path("renamed.bdf")).exists());
            std::fs::write(dir.path("original.bdf"), b"OTTOunsupported").unwrap();
            assert!(run_jobs(&plan, &Default::default(), &mut Vec::new()).is_err());
            assert_eq!(
                std::fs::read(dir.path("original.bdf")).unwrap(),
                b"OTTOunsupported"
            );
        }
        #[test]
        fn pcf_and_gzip_inputs_remain_immutable() {
            let dir = Directory::new();
            let font = bdf::read(SOURCE, &Default::default()).unwrap().0;
            let pcf_bytes = pcf::write(&font, &Default::default()).unwrap().0;
            let mut gzip =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(SOURCE).unwrap();
            let gzip_bytes = gzip.finish().unwrap();
            for (index, input) in [pcf_bytes, gzip_bytes].iter().enumerate() {
                std::fs::write(dir.path("original.bdf"), input).unwrap();
                let plan = Plan {
                    jobs: vec![job(&dir, &format!("renamed{index}.bdf"))],
                };
                run_jobs(&plan, &Default::default(), &mut Vec::new()).unwrap();
                assert_eq!(std::fs::read(dir.path("original.bdf")).unwrap(), *input);
                let output = std::fs::read_to_string(&plan.jobs[0].output).unwrap();
                assert!(output.contains("COPYRIGHT \"Copyright original\""));
            }
        }
        #[test]
        fn report_concatenation_keeps_every_native_entry_in_order() {
            let (_, read) = bdf::read(SOURCE, &Default::default()).unwrap();
            let mut font = bdf::read(SOURCE, &Default::default()).unwrap().0;
            let policy = rename::apply(
                &mut font,
                &rename::ExportName {
                    name: "New",
                    size: 8,
                    weight: "regular",
                    weight_capitalized: "Regular",
                    width: "normal",
                    spacing: "mono",
                    script_subset: None,
                },
                &Default::default(),
            )
            .unwrap();
            let (_, write) = bdf::write(&font, &Default::default()).unwrap();
            let encoded = serde_json::to_value(Reports([&read, &policy, &write])).unwrap();
            let mut combined = read;
            combined.extend(policy);
            combined.extend(write);
            assert_eq!(encoded, serde_json::to_value(combined).unwrap());
        }
    }
}

#[cfg(all(feature = "bdf", feature = "pcf", feature = "serde"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    enabled::run()
}
#[cfg(not(all(feature = "bdf", feature = "pcf", feature = "serde")))]
fn main() {
    eprintln!("phase6_catalogue_export_name requires bdf, pcf and serde features");
    std::process::exit(2);
}
