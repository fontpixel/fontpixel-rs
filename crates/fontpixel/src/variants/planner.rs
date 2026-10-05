//! Incremental regional planning on one working ledger.
//!
//! Base records borrow the base face. Alternative-only characters and new
//! alternates are retained as admitted copies (resolved metrics plus bitmap)
//! when they are first planned, so an alternative face is no longer needed once
//! its own pass ends: an owning caller may drop it and release its charge
//! before the next face is planned.
use super::*;

/// Where a planned record's glyph lives until the final order is built.
#[derive(Clone, Copy)]
enum Source<'b> {
    /// Borrowed from the base face; its defaults resolve on demand.
    Base(&'b Glyph),
    /// Index into the planner's admitted copies, with metrics resolved.
    Owned(u32),
}

/// Fields are declared before the leases that charge them, so implicit drops
/// release storage before its charge.
pub(super) struct Planner<'b> {
    base: &'b Font,
    names: Nested<String>,
    records: Vector<Source<'b>>,
    owned: Nested<Glyph>,
    cmap: BTreeMap<u32, u32>,
    cmap_lease: Lease,
    maps: Nested<BTreeMap<u32, u32>>,
    // Ordered (signature, record) buckets; full equality resolves collisions.
    signatures: BTreeSet<(u64, u32)>,
    signatures_lease: Lease,
}

impl<'b> Planner<'b> {
    /// Plan every base mapping.
    pub(super) fn new(
        base: &'b Font,
        alternatives: usize,
        limits: &LayoutLimits,
        budget: &Budget,
    ) -> Result<Self> {
        let mut planner = Self {
            base,
            names: Nested::with_capacity(budget, 0)?,
            records: Vector::new(budget)?,
            owned: Nested::with_capacity(budget, 0)?,
            cmap: BTreeMap::new(),
            cmap_lease: budget.reserve(0)?,
            maps: Nested::with_capacity(budget, alternatives)?,
            signatures: BTreeSet::new(),
            signatures_lease: budget.reserve(0)?,
        };
        for entry in effective(base) {
            let (cp, g) = entry?;
            limits
                .sfnt
                .glyphs(planner.records.len().saturating_add(2))?;
            sfnt::bounded(
                planner.cmap.len().saturating_add(1),
                limits.sfnt.max_cmap_mappings,
                "variant Unicode mappings",
            )?;
            let index = index(planner.records.len())?;
            planner
                .names
                .push(ledger::string(budget, GlyphName::base(cp))?)?;
            planner.records.push(Source::Base(g))?;
            ledger::tree::<u32, u32>(&mut planner.cmap_lease, budget, planner.cmap.len() + 1)?;
            planner.cmap.insert(cp, index);
        }
        Ok(planner)
    }

    /// Resolved glyph of a planned record, borrowing its bitmap.
    fn view(&self, i: u32) -> ledger::View<'_> {
        match self.records[i as usize] {
            Source::Base(g) => ledger::View::new(self.base, g),
            Source::Owned(k) => {
                let g = &self.owned[k as usize];
                ledger::View {
                    metrics: g.metrics.clone(),
                    bitmap: &g.bitmap,
                }
            }
        }
    }

    /// Retain an admitted copy of an alternative face's glyph as a new record.
    fn retain(
        &mut self,
        name: Owned<String>,
        view: ledger::View<'_>,
        budget: &Budget,
    ) -> Result<u32> {
        let i = index(self.records.len())?;
        let k = index(self.owned.len())?;
        self.owned.push(view.retain(budget)?)?;
        self.names.push(name)?;
        self.records.push(Source::Owned(k))?;
        Ok(i)
    }

    /// Plan one alternative face. Nothing borrowed from `font` outlives the call.
    pub(super) fn face(
        &mut self,
        font: &Font,
        label: Label,
        limits: &LayoutLimits,
        budget: &Budget,
    ) -> Result<()> {
        let label = ledger::string(budget, label)?;
        let mut substitutions = BTreeMap::<u32, u32>::new();
        let mut substitutions_lease = budget.reserve(0)?;
        for entry in effective(font) {
            let (cp, source) = entry?;
            if !self.cmap.contains_key(&cp) {
                limits.sfnt.glyphs(self.records.len().saturating_add(2))?;
                sfnt::bounded(
                    self.cmap.len().saturating_add(1),
                    limits.sfnt.max_cmap_mappings,
                    "variant Unicode mappings",
                )?;
            }
            let view = ledger::View::new(font, source);
            let Some(&base) = self.cmap.get(&cp) else {
                let name = ledger::string(budget, GlyphName::base(cp))?;
                let index = self.retain(name, view, budget)?;
                ledger::tree::<u32, u32>(&mut self.cmap_lease, budget, self.cmap.len() + 1)?;
                self.cmap.insert(cp, index);
                continue;
            };
            if view.matches(&self.view(base)) {
                continue;
            }
            let hash = view.signature();
            let existing = self
                .signatures
                .range((hash, 0)..=(hash, u32::MAX))
                .map(|&(_, i)| i)
                .find(|&i| view.matches(&self.view(i)));
            let replacement = if let Some(i) = existing {
                i
            } else {
                limits.sfnt.glyphs(self.records.len().saturating_add(2))?;
                // A glyph name's prefix before its first '.' is the unique base
                // name of its codepoint, so only alternates already routed from
                // this base can share a candidate name.
                let mut suffix = 0usize;
                let name = loop {
                    let name = ledger::string(
                        budget,
                        GlyphName {
                            cp,
                            label: Some((label.as_str(), suffix)),
                        },
                    )?;
                    if !self
                        .maps
                        .iter()
                        .filter_map(|m| m.get(&base))
                        .any(|&i| self.names[i as usize] == *name)
                    {
                        break name;
                    }
                    suffix += 1;
                };
                ledger::tree::<(u64, u32), ()>(
                    &mut self.signatures_lease,
                    budget,
                    self.signatures.len() + 1,
                )?;
                let i = self.retain(name, view, budget)?;
                self.signatures.insert((hash, i));
                i
            };
            ledger::tree::<u32, u32>(&mut substitutions_lease, budget, substitutions.len() + 1)?;
            substitutions.insert(base, replacement);
        }
        self.maps
            .push(Owned::from_parts(substitutions, substitutions_lease))?;
        limits.sfnt.glyphs(self.records.len().saturating_add(1))
    }

    /// Order glyphs, build exact locl routing and retain each planned glyph once.
    /// Only the faces' routing tags are read; their fonts may already be empty.
    pub(super) fn finish(
        self,
        languages: &[LanguageVariant],
        scripts: &[ScriptVariant],
        limits: &LayoutLimits,
        budget: &Budget,
    ) -> Result<Owned<Plan>> {
        let Self {
            base,
            names,
            records,
            owned,
            cmap,
            cmap_lease,
            maps,
            signatures,
            signatures_lease,
        } = self;
        drop(signatures);
        drop(signatures_lease);
        // Tong sorts by base name, then full alternate name; names are unique,
        // so an in-place unstable sort yields the same order without scratch.
        let n = records.len();
        limits.sfnt.glyphs(n.saturating_add(1))?;
        let mut order = Vector::<u32>::with_capacity(budget, n)?;
        for i in 0..n {
            order.push(index(i)?)?;
        }
        order.sort_unstable_by(|&a, &b| names[a as usize].cmp(&names[b as usize]));
        let mut ids = Vector::<u16>::with_capacity(budget, n)?;
        ids.resize(n, 0)?;
        for (rank, &i) in order.iter().enumerate() {
            ids[i as usize] = (rank + 1) as u16;
        }
        let mut gsub_lease = budget.reserve(0)?;
        let mut gsub = Gsub::default();
        {
            let mut lookups = Nested::<BTreeMap<u16, u16>>::with_capacity(budget, maps.len())?;
            for map in maps.iter() {
                let lease = ledger::reserve_tree::<u16, u16>(budget, map.len())?;
                let pairs = map
                    .iter()
                    .map(|(&a, &b)| (ids[a as usize], ids[b as usize]))
                    .collect();
                lookups.push(Owned::from_parts(pairs, lease))?;
            }
            drop(maps);
            let (lookups, mut lease) = lookups.into_owned()?.into_parts();
            gsub.lookups = lookups;
            gsub_lease.merge(&mut lease)?;
        }
        {
            let mut systems_lease = budget.reserve(0)?;
            let mut systems = BTreeMap::<(Tag, Option<Tag>), Vector<u16>>::new();
            for (i, v) in scripts.iter().enumerate() {
                for &script in &v.scripts {
                    validate_tag(script)?;
                    let key = (script, None);
                    if !systems.contains_key(&key) {
                        ledger::tree::<(Tag, Option<Tag>), Vector<u16>>(
                            &mut systems_lease,
                            budget,
                            systems.len() + 1,
                        )?;
                        systems.insert(key, Vector::new(budget)?);
                    }
                    if let Some(values) = systems.get_mut(&key) {
                        values.push((languages.len() + i) as u16)?;
                    }
                }
            }
            for (i, v) in languages.iter().enumerate() {
                validate_tag(v.language)?;
                for &script in &v.scripts {
                    validate_tag(script)?;
                    let key = (script, Some(v.language));
                    if systems.contains_key(&key) {
                        return Err(invalid("duplicate script/language variant"));
                    }
                    let inherited = systems.get(&(script, None)).map_or(&[][..], |v| &v[..]);
                    let mut values = Vector::with_capacity(budget, 1 + inherited.len())?;
                    values.push(i as u16)?;
                    values.extend(inherited)?;
                    ledger::tree::<(Tag, Option<Tag>), Vector<u16>>(
                        &mut systems_lease,
                        budget,
                        systems.len() + 1,
                    )?;
                    systems.insert(key, values);
                }
            }
            // Feature lists retained by the GSUB model, keyed by their lookups.
            let mut features = Nested::<Feature>::with_capacity(budget, 0)?;
            let mut feature_ids_lease = budget.reserve(0)?;
            let mut feature_keys = budget.reserve(0)?;
            let mut feature_ids = BTreeMap::<Vec<u16>, u16>::new();
            let mut scripts_lease = budget.reserve(0)?;
            let mut systems_heap = budget.reserve(0)?;
            // Preserve explicit empty language systems: they prevent accidental fallback.
            for ((tag, language), routes) in systems {
                let retained = routes
                    .iter()
                    .filter(|&&id| !gsub.lookups[usize::from(id)].is_empty())
                    .count();
                let mut lookups = Vector::with_capacity(budget, retained)?;
                for &id in routes.iter() {
                    if !gsub.lookups[usize::from(id)].is_empty() {
                        lookups.push(id)?;
                    }
                }
                drop(routes);
                let selected = if lookups.is_empty() {
                    Vector::new(budget)?
                } else {
                    let id = if let Some(&id) = feature_ids.get(&lookups[..]) {
                        id
                    } else {
                        let id = sfnt::to16(features.len())?;
                        let mut key_lease = budget.reserve_array::<u16>(lookups.len())?;
                        ledger::tree::<Vec<u16>, u16>(
                            &mut feature_ids_lease,
                            budget,
                            feature_ids.len() + 1,
                        )?;
                        feature_keys.merge(&mut key_lease)?;
                        feature_ids.insert(lookups.to_vec(), id);
                        let (lookups, lease) = lookups.into_parts();
                        features.push(Owned::from_parts(
                            Feature {
                                tag: *b"locl",
                                lookups,
                            },
                            lease,
                        ))?;
                        id
                    };
                    let mut selected = Vector::with_capacity(budget, 1)?;
                    selected.push(id)?;
                    selected
                };
                let (features, mut lease) = selected.into_parts();
                if !gsub.scripts.contains_key(&tag) {
                    ledger::tree::<Tag, Script>(
                        &mut scripts_lease,
                        budget,
                        gsub.scripts.len() + 1,
                    )?;
                }
                let s = gsub.scripts.entry(tag).or_insert_with(|| Script {
                    default: Some(LanguageSystem::default()),
                    languages: Default::default(),
                });
                let value = LanguageSystem {
                    required: None,
                    features,
                };
                if let Some(lang) = language {
                    ledger::grow::<Tag, LanguageSystem>(
                        &mut systems_heap,
                        budget,
                        s.languages.len(),
                    )?;
                    s.languages.insert(lang, value);
                } else {
                    s.default = Some(value);
                }
                systems_heap.merge(&mut lease)?;
            }
            drop(feature_ids);
            drop(feature_keys);
            drop(feature_ids_lease);
            let (features, mut lease) = features.into_owned()?.into_parts();
            gsub.features = features;
            for l in [&mut lease, &mut scripts_lease, &mut systems_heap] {
                gsub_lease.merge(l)?;
            }
        }
        // Prune empty lookups/features through the identity subset, then release
        // the unpruned model and mapping by dropping them before the encode check.
        let subset = {
            let mapping_lease = ledger::reserve_tree::<u16, u16>(budget, n + 1)?;
            let mapping = (0..=n as u16).map(|i| (i, i)).collect();
            let subset = gsub.subset_with_budget(&mapping, budget)?;
            drop(mapping);
            drop(mapping_lease);
            subset
        };
        drop(gsub);
        drop(gsub_lease);
        let (gsub, mut lease) = subset.into_parts();
        drop(gsub.encode_with_budget((n + 1) as u16, limits, budget)?);
        let mut cmap_out_lease = ledger::reserve_tree::<u32, u16>(budget, cmap.len())?;
        let cmap_out = cmap
            .iter()
            .map(|(&cp, &i)| (cp, ids[i as usize]))
            .collect::<BTreeMap<_, _>>();
        drop(cmap);
        drop(cmap_lease);
        drop(ids);
        // Retain each planned glyph once, in final order, with an exact array.
        let mut glyphs = Nested::<(String, Glyph)>::with_capacity(budget, n)?;
        for &i in order.iter() {
            let view = match records[i as usize] {
                Source::Base(g) => ledger::View::new(base, g),
                Source::Owned(k) => {
                    let g = &owned[k as usize];
                    ledger::View {
                        metrics: g.metrics.clone(),
                        bitmap: &g.bitmap,
                    }
                }
            };
            let name = ledger::string(budget, &names[i as usize])?;
            glyphs.push(ledger::record(name, view.retain(budget)?)?)?;
        }
        drop(order);
        drop(records);
        drop(owned);
        drop(names);
        let (glyphs, mut glyphs_lease) = glyphs.into_owned()?.into_parts();
        lease.merge(&mut cmap_out_lease)?;
        lease.merge(&mut glyphs_lease)?;
        Ok(Owned::from_parts(
            Plan {
                glyphs,
                cmap: cmap_out,
                gsub,
            },
            lease,
        ))
    }
}
