use core::fmt;
pub(crate) use core::sync::atomic::*;

#[derive(Default)]
pub(crate) struct AtomicF32(AtomicU32);

impl AtomicF32 {
    /// This is a separate function because `f32::to_bits` is not yet stable as
    /// a `const fn`, and we would like a const constructor.
    #[inline]
    #[must_use]
    pub(crate) const fn zero() -> Self {
        Self(AtomicU32::new(0))
    }

    #[inline]
    #[must_use]
    pub(crate) fn new(f: f32) -> Self {
        Self(AtomicU32::new(f.to_bits()))
    }

    #[inline]
    #[must_use]
    pub(crate) fn load(&self, order: Ordering) -> f32 {
        let bits = self.0.load(order);
        f32::from_bits(bits)
    }

    #[inline]
    pub(crate) fn store(&self, f: f32, order: Ordering) {
        let bits = f.to_bits();
        self.0.store(bits, order)
    }

    // TODO(eliza): add CAS, etc, if needed.
}

impl fmt::Debug for AtomicF32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AtomicF32")
            .field(&self.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for AtomicF32 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f32(self.load(Ordering::Relaxed))
    }
}
