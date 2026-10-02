use std::marker::PhantomData;

use crate::SvidGenerator;

/// Marker trait associating a zero-sized "kind" type with an SVID tag and a
/// concrete `Id` newtype.
///
/// Implemented by `#[derive(svid::Svid)]` for each variant's `<Variant>Marker`,
/// e.g. `impl SvidKind for UserIdMarker { type Id = UserId; const TAG: u8 = ... }`.
pub trait SvidKind {
    type Id;
    const TAG: u8;
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
    K::Id: From<i64>,
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
        let id = if self.monotonic {
            SvidGenerator::generate_monotonic(K::TAG, self.is_client)
        } else {
            SvidGenerator::generate(K::TAG, self.is_client)
        };
        K::Id::from(id)
    }
}

/// Type-driven dispatch trait implemented by the `Svid` derive on the registry
/// it generates: lets callers write `let u: UserId = reg.generate_id();`.
pub trait GenerateId<T> {
    fn generate(&self) -> T;
}
