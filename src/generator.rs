use std::marker::PhantomData;

use crate::SvidGenerator;

/// Marker trait associating a zero-sized "kind" type with an SVID tag and a
/// concrete `Id` newtype.
///
/// Implemented by `#[derive(svid::Svid)]` for each variant's `<Variant>Marker`,
/// e.g. `impl SvidKind for UserIdMarker { type Id = UserId; const TAG: u16 = ... }`.
pub trait SvidKind {
    type Id;
    type Raw: SvidValue;
    const TAG: u16;
}

/// Typed wrapper around `SvidGenerator` that returns the strongly-typed `Id`
/// associated with `K`.
pub struct IdGenerator<K> {
    is_client: bool,
    monotonic: bool,
    _phantom: PhantomData<K>,
}

impl<K: SvidKind> IdGenerator<K>
where
    K::Id: From<K::Raw>,
{
    pub fn new(is_client: bool) -> Self {
        Self {
            is_client,
            monotonic: false,
            _phantom: PhantomData,
        }
    }

    /// Generator backed by the process-wide monotonic [`Sequencer`](crate::Sequencer).
    pub fn new_monotonic(is_client: bool) -> Self {
        Self {
            is_client,
            monotonic: true,
            _phantom: PhantomData,
        }
    }

    pub fn generate_id(&self) -> K::Id {
        let id = K::Raw::generate(K::TAG, self.is_client, self.monotonic);
        K::Id::from(id)
    }
}

/// Type-driven dispatch trait implemented by the `Svid` derive on the registry
/// it generates: lets callers write `let u: UserId = reg.generate_id();`.
pub trait GenerateId<T> {
    fn generate(&self) -> T;
}

/// Storage representation used by a typed ID registry.
pub trait SvidValue {
    fn generate(tag: u16, is_client: bool, monotonic: bool) -> Self;
}
impl SvidValue for i64 {
    fn generate(tag: u16, is_client: bool, monotonic: bool) -> Self {
        if monotonic {
            SvidGenerator::generate_monotonic(tag, is_client)
        } else {
            SvidGenerator::generate(tag, is_client)
        }
    }
}
impl SvidValue for crate::Svid128 {
    fn generate(tag: u16, is_client: bool, monotonic: bool) -> Self {
        assert!(
            !monotonic,
            "monotonic generation is only available for SVID64"
        );
        // Entity IDs retain independent random bits, including across instances.
        Self::generate_with_source(tag, is_client)
    }
}
