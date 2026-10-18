//! Strict single-pass typed decoding of an already byte-bounded SQLite row.
//! No Value tree, permissive model roundtrip, or source capability is created.

use crate::{
    capture::Deadline,
    model::{Coverage, Diagnostic, Entity, Evidence, Graph, Observation, valid_id},
    retrieval_capacity::MAX_GRAPH_BYTES,
    retrieval_scopes::{RetrievalHardError, RetrievalResult, check_deadline},
};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{borrow::Cow, cell::Cell, collections::BTreeMap, fmt, marker::PhantomData};

struct Context<'d> {
    deadline: &'d Deadline,
    failure: Cell<Option<RetrievalHardError>>,
}
impl Context<'_> {
    fn error<E: serde::de::Error>(&self, cause: RetrievalHardError) -> E {
        if self.failure.get().is_none() {
            self.failure.set(Some(cause));
        }
        E::custom("saved graph rejected")
    }
    fn check<E: serde::de::Error>(&self) -> Result<(), E> {
        check_deadline(self.deadline).map_err(|cause| self.error(cause))
    }
    fn schema<E: serde::de::Error>(&self) -> E {
        self.error(RetrievalHardError::SourceIntegrity("saved_graph_schema"))
    }
}

trait Decode<'de>: Sized {
    fn decode<D: serde::Deserializer<'de>>(
        context: &Context<'_>,
        deserializer: D,
    ) -> Result<Self, D::Error>;
}
struct Seed<'a, 'd, T> {
    context: &'a Context<'d>,
    allow: bool,
    marker: PhantomData<T>,
}
impl<'a, 'd, T> Seed<'a, 'd, T> {
    fn new(context: &'a Context<'d>) -> Self {
        Self {
            context,
            allow: true,
            marker: PhantomData,
        }
    }
}
impl<'de, T: Decode<'de>> DeserializeSeed<'de> for Seed<'_, '_, T> {
    type Value = T;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        self.context.check()?;
        if !self.allow {
            return Err(self
                .context
                .error(RetrievalHardError::Capacity("graph_collection_items")));
        }
        T::decode(self.context, d)
    }
}

struct TextVisitor<'a, 'd> {
    context: &'a Context<'d>,
    limit: usize,
}
impl<'de> Visitor<'de> for TextVisitor<'_, '_> {
    type Value = Cow<'de, str>;
    fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("bounded saved graph string")
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, text: &'de str) -> Result<Self::Value, E> {
        self.context.check()?;
        if text.len() > self.limit {
            return Err(self.context.schema());
        }
        Ok(Cow::Borrowed(text))
    }
    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
        self.context.check()?;
        if text.len() > self.limit {
            return Err(self.context.schema());
        }
        let mut owned = String::new();
        owned.try_reserve_exact(text.len()).map_err(|_| {
            self.context
                .error(RetrievalHardError::Capacity("graph_decode_allocation"))
        })?;
        owned.push_str(text);
        Ok(Cow::Owned(owned))
    }
}
impl<'de> Decode<'de> for Cow<'de, str> {
    fn decode<D: serde::Deserializer<'de>>(context: &Context<'_>, d: D) -> Result<Self, D::Error> {
        d.deserialize_str(TextVisitor {
            context,
            limit: MAX_GRAPH_BYTES,
        })
    }
}
impl<'de> Decode<'de> for String {
    fn decode<D: serde::Deserializer<'de>>(context: &Context<'_>, d: D) -> Result<Self, D::Error> {
        let text = Cow::decode(context, d)?;
        match text {
            Cow::Owned(text) => Ok(text),
            Cow::Borrowed(text) => {
                let mut owned = String::new();
                owned.try_reserve_exact(text.len()).map_err(|_| {
                    context.error(RetrievalHardError::Capacity("graph_decode_allocation"))
                })?;
                owned.push_str(text);
                Ok(owned)
            }
        }
    }
}
macro_rules! scalar {
    ($($kind:ty),+ $(,)?) => {$ (
        impl<'de> Decode<'de> for $kind {
            fn decode<D: serde::Deserializer<'de>>(
                _context: &Context<'_>, d: D,
            ) -> Result<Self, D::Error> {
                serde::Deserialize::deserialize(d)
            }
        }
    )+};
}
scalar!(usize, bool);

impl<'de, T: Decode<'de>> Decode<'de> for Option<T> {
    fn decode<D: serde::Deserializer<'de>>(context: &Context<'_>, d: D) -> Result<Self, D::Error> {
        struct Optional<'a, 'd, T>(&'a Context<'d>, PhantomData<T>);
        impl<'de, T: Decode<'de>> Visitor<'de> for Optional<'_, '_, T> {
            type Value = Option<T>;
            fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                out.write_str("optional saved graph field")
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                self.0.check()?;
                Ok(None)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                self.visit_none()
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<Self::Value, D::Error> {
                Seed::<T>::new(self.0).deserialize(d).map(Some)
            }
        }
        d.deserialize_option(Optional::<T>(context, PhantomData))
    }
}

trait ItemLimit {
    const LIMIT: usize;
    const CODE: &'static str;
}
impl ItemLimit for Entity {
    const LIMIT: usize = 100_000;
    const CODE: &'static str = "graph_entities";
}
impl ItemLimit for Observation {
    const LIMIT: usize = 1_000_000;
    const CODE: &'static str = "graph_observations";
}
// A JSON object or string needs at least two bytes plus its array comma.
// These derived bounds exclude no value admitted by the raw byte ceiling.
macro_rules! wire_limited_items {
    ($($kind:ty),+ $(,)?) => {$ (
        impl ItemLimit for $kind {
            const LIMIT: usize = (MAX_GRAPH_BYTES + 1) / 3;
            const CODE: &'static str = "graph_collection_items";
        }
    )+};
}
wire_limited_items!(Evidence, Diagnostic, String);

impl<'de, T: Decode<'de> + ItemLimit> Decode<'de> for Vec<T> {
    fn decode<D: serde::Deserializer<'de>>(context: &Context<'_>, d: D) -> Result<Self, D::Error> {
        struct List<'a, 'd, T>(&'a Context<'d>, PhantomData<T>);
        impl<'de, T: Decode<'de> + ItemLimit> Visitor<'de> for List<'_, '_, T> {
            type Value = Vec<T>;
            fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                out.write_str("bounded saved graph collection")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut output = Vec::new();
                loop {
                    // SeqAccess calls this seed only if another element exists.
                    // At the ceiling the seed fails before that element decodes.
                    struct Next<'a, 'd, T>(&'a Context<'d>, bool, PhantomData<T>);
                    impl<'de, T: Decode<'de> + ItemLimit> DeserializeSeed<'de> for Next<'_, '_, T> {
                        type Value = T;
                        fn deserialize<D: serde::Deserializer<'de>>(
                            self,
                            d: D,
                        ) -> Result<T, D::Error> {
                            self.0.check()?;
                            if !self.1 {
                                return Err(self.0.error(RetrievalHardError::Capacity(T::CODE)));
                            }
                            T::decode(self.0, d)
                        }
                    }
                    let Some(item) = input.next_element_seed(Next::<T>(
                        self.0,
                        output.len() < T::LIMIT,
                        PhantomData,
                    ))?
                    else {
                        break;
                    };
                    if output.len() == output.capacity() {
                        let next = output.capacity().saturating_mul(2).max(4).min(T::LIMIT);
                        output.try_reserve_exact(next - output.len()).map_err(|_| {
                            self.0
                                .error(RetrievalHardError::Capacity("graph_decode_allocation"))
                        })?;
                    }
                    output.push(item);
                }
                Ok(output)
            }
        }
        d.deserialize_seq(List::<T>(context, PhantomData))
    }
}

// Decoded dictionary keys must remain unique, including JSON escape aliases.
impl<'de> Decode<'de> for BTreeMap<String, usize> {
    fn decode<D: serde::Deserializer<'de>>(context: &Context<'_>, d: D) -> Result<Self, D::Error> {
        struct Dictionary<'a, 'd>(&'a Context<'d>);
        impl<'de> Visitor<'de> for Dictionary<'_, '_> {
            type Value = BTreeMap<String, usize>;
            fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                out.write_str("unique decoded saved graph dictionary keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut output = BTreeMap::new();
                loop {
                    // Minimal pair is "":0 plus a comma, five wire bytes.
                    let key = input.next_key_seed(Seed::<String> {
                        context: self.0,
                        allow: output.len() < (MAX_GRAPH_BYTES + 1) / 5,
                        marker: PhantomData,
                    })?;
                    let Some(key) = key else { break };
                    if output.contains_key(&key) {
                        return Err(self.0.schema());
                    }
                    let value = input.next_value_seed(Seed::<usize>::new(self.0))?;
                    output.insert(key, value);
                }
                Ok(output)
            }
        }
        d.deserialize_map(Dictionary(context))
    }
}

// The macro constructs the actual model directly. All present fields, including
// optional null fields, have independent duplicate detection before their value.
macro_rules! object {
    ($kind:ident, required {$($field:ident : $ty:ty => $wire:literal),* $(,)?},
        optional {$($optional:ident : $optional_ty:ty => $optional_wire:literal),* $(,)?}) => {
        impl<'de> Decode<'de> for $kind {
            fn decode<D: serde::Deserializer<'de>>(
                context: &Context<'_>, d: D,
            ) -> Result<Self, D::Error> {
                struct Object<'a, 'd>(&'a Context<'d>);
                impl<'de> Visitor<'de> for Object<'_, '_> {
                    type Value = $kind;
                    fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                        out.write_str("strict saved graph object")
                    }
                    fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<$kind, A::Error> {
                        $(let mut $field: Option<$ty> = None;)*
                        $(let mut $optional: Option<Option<$optional_ty>> = None;)*
                        while let Some(key) = input.next_key_seed(Seed::<Cow<'de, str>>::new(self.0))? {
                            match key.as_ref() {
                                $($wire => {
                                    if $field.is_some() { return Err(self.0.schema()); }
                                    $field = Some(input.next_value_seed(Seed::<$ty>::new(self.0))?);
                                })*
                                $($optional_wire => {
                                    if $optional.is_some() { return Err(self.0.schema()); }
                                    $optional = Some(input.next_value_seed(Seed::<Option<$optional_ty>>::new(self.0))?);
                                })*
                                _ => return Err(self.0.schema()),
                            }
                        }
                        self.0.check()?;
                        Ok($kind {
                            $($field: $field.ok_or_else(|| self.0.schema::<A::Error>())?,)*
                            $($optional: $optional.unwrap_or(None),)*
                        })
                    }
                }
                d.deserialize_struct(stringify!($kind), &[$($wire,)* $($optional_wire,)*], Object(context))
            }
        }
    };
}
object!(Graph, required {
    snapshot_id: String => "snapshotId",
    entities: Vec<Entity> => "entities",
    observations: Vec<Observation> => "observations",
    evidence: Vec<Evidence> => "evidence",
    diagnostics: Vec<Diagnostic> => "diagnostics",
    coverage: Coverage => "coverage",
}, optional {});
object!(Entity, required {
    id: String => "id", key: String => "key", snapshot_id: String => "snapshotId",
    path: String => "path", name: String => "name", qualified_name: String => "qualifiedName",
    kind: String => "kind", language: String => "language", signature: String => "signature",
    start_byte: usize => "startByte", end_byte: usize => "endByte",
    start_line: usize => "startLine", end_line: usize => "endLine", producer: String => "producer",
}, optional {
    name_start_byte: usize => "nameStartByte", name_end_byte: usize => "nameEndByte",
    parent_key: String => "parentKey",
});
object!(Observation, required {
    id: String => "id", snapshot_id: String => "snapshotId", source: String => "source",
    target_label: String => "targetLabel", relation: String => "relation", resolution: String => "resolution",
    evidence_ids: Vec<String> => "evidenceIds", producer: String => "producer",
}, optional { target: String => "target" });
object!(Evidence, required {
    id: String => "id", snapshot_id: String => "snapshotId", path: String => "path",
    content_hash: String => "contentHash", start_byte: usize => "startByte", end_byte: usize => "endByte",
    start_line: usize => "startLine", end_line: usize => "endLine", anchor: String => "anchor",
    trust: String => "trust", producer: String => "producer",
}, optional {});
object!(Diagnostic, required {
    path: String => "path", start_byte: usize => "startByte", message: String => "message",
    severity: String => "severity", producer: String => "producer",
}, optional {});
object!(Coverage, required {
    indexed_files: usize => "indexedFiles", source_files: usize => "sourceFiles",
    languages: BTreeMap<String, usize> => "languages", unresolved: usize => "unresolved",
    omissions: usize => "omissions", exhaustive: bool => "exhaustive", semantic: bool => "semantic",
    limitations: Vec<String> => "limitations",
}, optional {});

pub(super) fn decode(bytes: &[u8], deadline: &Deadline) -> RetrievalResult<Graph> {
    check_deadline(deadline)?;
    if bytes.len() > MAX_GRAPH_BYTES {
        return Err(RetrievalHardError::Capacity("graph_json_bytes"));
    }
    let context = Context {
        deadline,
        failure: Cell::new(None),
    };
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let result = Seed::<Graph>::new(&context)
        .deserialize(&mut parser)
        .and_then(|graph| parser.end().map(|()| graph));
    check_deadline(deadline)?;
    let graph = result.map_err(|_| {
        context
            .failure
            .get()
            .unwrap_or(RetrievalHardError::SourceIntegrity("saved_graph_schema"))
    })?;
    validate(&graph, deadline)?;
    Ok(graph)
}

fn validate(graph: &Graph, deadline: &Deadline) -> RetrievalResult<()> {
    let invalid = || RetrievalHardError::SourceIntegrity("saved_graph_metadata");
    if !valid_id(&graph.snapshot_id) {
        return Err(invalid());
    }
    for entity in &graph.entities {
        check_deadline(deadline)?;
        if !valid_id(&entity.id)
            || entity.snapshot_id != graph.snapshot_id
            || crate::paths::normalized_syntax(&entity.path).is_err()
            || entity.start_byte > entity.end_byte
            || entity.start_line > entity.end_line
            || match (entity.name_start_byte, entity.name_end_byte) {
                (None, None) => false,
                (Some(start), Some(end)) => {
                    start > end || start < entity.start_byte || end > entity.end_byte
                }
                _ => true,
            }
        {
            return Err(invalid());
        }
    }
    for observation in &graph.observations {
        check_deadline(deadline)?;
        if !valid_id(&observation.id)
            || observation.snapshot_id != graph.snapshot_id
            || !valid_id(&observation.source)
            || observation.target.as_ref().is_some_and(|id| !valid_id(id))
        {
            return Err(invalid());
        }
        for id in &observation.evidence_ids {
            check_deadline(deadline)?;
            if !valid_id(id) {
                return Err(invalid());
            }
        }
    }
    for evidence in &graph.evidence {
        check_deadline(deadline)?;
        if !valid_id(&evidence.id)
            || evidence.snapshot_id != graph.snapshot_id
            || !valid_id(&evidence.content_hash)
            || crate::paths::normalized_syntax(&evidence.path).is_err()
            || evidence.start_byte > evidence.end_byte
            || evidence.start_line > evidence.end_line
        {
            return Err(invalid());
        }
    }
    for diagnostic in &graph.diagnostics {
        check_deadline(deadline)?;
        if crate::paths::normalized_syntax(&diagnostic.path).is_err() {
            return Err(invalid());
        }
    }
    check_deadline(deadline)
}
