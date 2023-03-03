//! A statically-constructable, dynamically-initialized fixed size array of values.
use core::{
    cell::UnsafeCell,
    fmt,
    iter::{DoubleEndedIterator, FusedIterator},
    mem::MaybeUninit,
    ptr, slice,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::*},
};

#[cfg(feature = "serde")]
use serde::{
    ser::{SerializeMap, SerializeSeq},
    Serialize, Serializer,
};

/// A statically-constructed but dynamically-initialized array of up to
/// `CAPACITY` `T`-typed values.
pub struct Registry<T, const CAPACITY: usize> {
    values: [Slot<T>; CAPACITY],
    next: AtomicUsize,
}

/// A [`Registry`] of `(K, V)` pairs.
pub struct RegistryMap<K, V, const CAPACITY: usize>(Registry<(K, V), CAPACITY>);

/// An iterator over a [`Registry`].
#[derive(Debug)]
pub struct Iter<'registry, T> {
    slots: slice::Iter<'registry, Slot<T>>,
}

/// An iterator over a [`RegistryMap`]'s entries.
#[derive(Debug)]
pub struct Entries<'registry, K, V> {
    slots: slice::Iter<'registry, Slot<(K, V)>>,
}

/// An iterator over a [`RegistryMap`]'s keys.
#[derive(Debug)]
pub struct Keys<'registry, K, V> {
    slots: slice::Iter<'registry, Slot<(K, V)>>,
}

/// An iterator over a [`RegistryMap`]'s values.
#[derive(Debug)]
pub struct Values<'registry, K, V> {
    slots: slice::Iter<'registry, Slot<(K, V)>>,
}

struct Slot<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    initialized: AtomicBool,
}

impl<T, const CAPACITY: usize> Registry<T, CAPACITY> {
    const NEW_SLOT: Slot<T> = Slot {
        value: UnsafeCell::new(MaybeUninit::uninit()),
        initialized: AtomicBool::new(false),
    };

    /// Returns a new `Registry` which can store up to `CAPACITY` values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            values: [Self::NEW_SLOT; CAPACITY],
            next: AtomicUsize::new(0),
        }
    }

    /// Store `value` in this registry, returning a reference to the stored value.
    ///
    /// # Panics
    ///
    /// - If the registry has already been filled to capacity.
    ///
    /// The [`try_register`] method returns a [`Result`] if the registry is
    /// full, instead.
    #[track_caller]
    pub fn register<'registry>(&'registry self, value: T) -> &'registry T {
        match self.try_register(value) {
            Ok(slot) => slot,
            Err(_) => panic!("this registry can contain only {CAPACITY} values"),
        }
    }

    pub fn try_register<'registry>(&'registry self, value: T) -> Result<&'registry T, T> {
        let idx = self.next.fetch_add(1, AcqRel);

        let Some(slot) = self.values.get(idx) else {
            return Err(value);
        };
        assert!(!slot.initialized.load(Acquire), "slot already initialized!");

        let init = unsafe {
            // Safety: we have exclusive access to the slot.
            let uninit = &mut *slot.value.get();
            ptr::write(uninit.as_mut_ptr(), value);
            uninit.assume_init_ref()
        };

        let _was_init = slot.initialized.swap(true, AcqRel);
        debug_assert!(
            !_was_init,
            "slot initialized while we were initializing it, wtf!"
        );

        // value initialized!
        Ok(init)
    }

    pub fn register_default<'registry>(&'registry self) -> Option<&'registry T>
    where
        T: Default,
    {
        self.try_register(T::default()).ok()
    }

    #[must_use]
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            slots: self.values.iter(),
        }
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.next.load(Acquire)
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        CAPACITY
    }
}

unsafe impl<T: Send, const CAPACITY: usize> Send for Registry<T, CAPACITY> {}
unsafe impl<T: Sync, const CAPACITY: usize> Sync for Registry<T, CAPACITY> {}

impl<T, const CAPACITY: usize> fmt::Debug for Registry<T, CAPACITY>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'registry, T, const CAPACITY: usize> IntoIterator for &'registry Registry<T, CAPACITY> {
    type Item = &'registry T;
    type IntoIter = Iter<'registry, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(feature = "serde")]
impl<T, const CAPACITY: usize> Serialize for Registry<T, CAPACITY>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for value in self.iter() {
            seq.serialize_element(value)?;
        }
        seq.end()
    }
}

// === impl RegistryMap ===

impl<K, V, const CAPACITY: usize> RegistryMap<K, V, CAPACITY> {
    #[must_use]
    pub const fn new() -> Self {
        Self(Registry::new())
    }

    pub fn register_with<'registry>(
        &'registry self,
        key: K,
        init: impl FnOnce() -> V,
    ) -> Option<&'registry V>
    where
        K: PartialEq,
    {
        for (k, v) in self.iter() {
            // already exists!
            if &key == k {
                return Some(v);
            }
        }

        self.0.try_register((key, init())).ok().map(|(_, val)| val)
    }

    pub fn register_default<'registry>(&'registry self, key: K) -> Option<&'registry V>
    where
        K: PartialEq,
        V: Default,
    {
        self.register_with(key, V::default)
    }

    pub fn register<'registry>(&'registry self, key: K, value: V) -> Option<&'registry V>
    where
        K: PartialEq,
    {
        self.register_with(key, move || value)
    }

    /// Returns an iterator that borrows the key-value pairs in this
    /// `RegistryMap`.
    #[must_use]
    #[inline]
    pub fn iter(&self) -> Entries<'_, K, V> {
        Entries {
            slots: self.0.values.iter(),
        }
    }

    /// Returns an iterator that borrows the `K`-typed keys in this
    /// `RegistryMap`.
    #[must_use]
    #[inline]
    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys {
            slots: self.0.values.iter(),
        }
    }

    /// Returns an iterator that borrows the `V`-typed values in this
    /// `RegistryMap`.
    #[must_use]
    #[inline]
    pub fn values(&self) -> Values<'_, K, V> {
        Values {
            slots: self.0.values.iter(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl<K, V, const CAPACITY: usize> fmt::Debug for RegistryMap<K, V, CAPACITY>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(feature = "serde")]
impl<K, V, const CAPACITY: usize> Serialize for RegistryMap<K, V, CAPACITY>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self.iter() {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

// === impl Iter ===

impl<'registry, T> Iterator for Iter<'registry, T> {
    type Item = &'registry T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next()?;
            // skip over uninitialized slots.
            if let Some(value) = slot.get() {
                return Some(value);
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.slots.size_hint()
    }
}

impl<'registry, T> FusedIterator for Iter<'registry, T> {}

impl<'registry, T> DoubleEndedIterator for Iter<'registry, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next_back()?;
            if let Some(value) = slot.get() {
                return Some(value);
            }
        }
    }
}

// === impl Entries ===

impl<'registry, K, V> Iterator for Entries<'registry, K, V> {
    type Item = (&'registry K, &'registry V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next()?;
            // skip over uninitialized slots.
            if let Some((ref key, ref value)) = slot.get() {
                return Some((key, value));
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.slots.size_hint()
    }
}

impl<'registry, K, V> FusedIterator for Entries<'registry, K, V> {}

impl<'registry, K, V> DoubleEndedIterator for Entries<'registry, K, V> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next_back()?;
            if let Some((ref key, ref value)) = slot.get() {
                return Some((key, value));
            }
        }
    }
}

// === impl Keys ===

impl<'registry, K, V> Iterator for Keys<'registry, K, V> {
    type Item = &'registry K;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next()?;
            // skip over uninitialized slots.
            if let Some((ref key, _)) = slot.get() {
                return Some(key);
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.slots.size_hint()
    }
}

impl<'registry, K, V> FusedIterator for Keys<'registry, K, V> {}

impl<'registry, K, V> DoubleEndedIterator for Keys<'registry, K, V> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next_back()?;
            if let Some((ref key, _)) = slot.get() {
                return Some(key);
            }
        }
    }
}

// === impl Values ===

impl<'registry, K, V> Iterator for Values<'registry, K, V> {
    type Item = &'registry V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next()?;
            // skip over uninitialized slots.
            if let Some((_, ref value)) = slot.get() {
                return Some(value);
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.slots.size_hint()
    }
}

impl<'registry, K, V> FusedIterator for Values<'registry, K, V> {}

impl<'registry, K, V> DoubleEndedIterator for Values<'registry, K, V> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let slot = self.slots.next_back()?;
            if let Some((_, value)) = slot.get() {
                return Some(value);
            }
        }
    }
}

// === impl Slot ==

impl<T> Slot<T> {
    fn get(&self) -> Option<&T> {
        if !self.initialized.load(Acquire) {
            return None;
        }

        unsafe {
            // Safety: we just checked the bit that tracks whether this value
            // was initialized.
            Some((&*self.value.get()).assume_init_ref())
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut tuple = f.debug_tuple("Slot");
        match self.get() {
            Some(value) => tuple.field(value).finish(),
            None => tuple.field(&"<uninitialized>").finish(),
        }
    }
}
