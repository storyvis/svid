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
    _phantom: PhantomData<K>,
}

impl<K: SvidKind> IdGenerator<K>
where
    K::Id: From<i64>,
{
    pub fn new(is_client: bool) -> Self {
        Self {
            is_client,
            _phantom: PhantomData,
        }
    }

    pub fn generate_id(&self) -> K::Id {
        K::Id::from(SvidGenerator::generate(K::TAG, self.is_client))
    }
}

/// Type-driven dispatch trait implemented by the `Svid` derive on the registry
/// it generates: lets callers write `let u: UserId = reg.generate_id();`.
pub trait GenerateId<T> {
    fn generate(&self) -> T;
}
