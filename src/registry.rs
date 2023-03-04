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
    /// - If the registry is [full].
    ///
    /// The [`try_register`] method returns a [`Result`] if the registry is
    /// full, instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::Registry;
    ///
    /// static REGISTRY: Registry<&'static str, 4> = Registry::new();
    ///
    /// let value1 = REGISTRY.register("foo");
    /// assert_eq!(value1, &"foo");
    /// ```
    ///
    /// This method panics once the registry is [full]:
    ///
    /// ```should_panic
    /// use tinymetrics::registry::Registry;
    ///
    /// // This registry can only store 4 values.
    /// static REGISTRY: Registry<usize, 4> = Registry::new();
    ///
    /// for i in 1..=4 {
    ///     REGISTRY.register(i);
    /// }
    ///
    /// // Registering the 5th value will panic:
    /// REGISTRY.register(5);
    /// ```
    ///
    /// [full]: Registry::is_full
    #[track_caller]
    pub fn register(&self, value: T) -> &T {
        match self.try_register(value) {
            Ok(slot) => slot,
            Err(_) => panic!("this registry can contain only {CAPACITY} values"),
        }
    }

    /// Attempt to store `value` in this registry, returning a reference to the
    /// stored value if it is successfully registered, or the original `value`
    /// if the registry is [full].
    ///
    /// # Returns
    ///
    /// - [`Ok`]`(&'registry T)` referencing the value stored in the registry,
    ///   if the value was registered.
    /// - [`Err`]`(T)` if the registry is [full].
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::Registry;
    ///
    /// static REGISTRY: Registry<&'static str, 2> = Registry::new();
    ///
    /// let value1 = REGISTRY.try_register("foo").expect("registry has capacity");
    /// assert_eq!(value1, &"foo");
    ///
    /// let value2 = REGISTRY.try_register("bar").expect("registry has capacity");
    /// assert_eq!(value2, &"bar");
    ///
    /// // Now that the registry is full, `try_register` returns an error
    /// // containing the original value:
    /// assert!(REGISTRY.try_register("baz").is_err());
    /// ```
    ///
    /// [full]: Self::is_full
    pub fn try_register(&self, value: T) -> Result<&T, T> {
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

    /// Attempt to store the value of `T::default` in this registry, returning a
    /// reference to the stored value if it is successfully registered, or `None`
    /// if the registry is [full].
    ///
    /// # Returns
    ///
    /// - [`Some`]`(&'registry T)` referencing the value stored in the registry,
    ///   if the value was registered.
    /// - [`None`] if the registry is [full].
    ///
    /// [full]: Self::is_full
    pub fn try_register_default(&self) -> Option<&T>
    where
        T: Default,
    {
        self.try_register(T::default()).ok()
    }

    /// Returns an iterator over all the entries currently stored in this
    /// `Registry`.
    #[must_use]
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            slots: self.values.iter(),
        }
    }

    /// Returns the number of entries currently stored in this `Registry`.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.next.load(Acquire)
    }

    /// Returns `true` if _no_ entries pairs are currently stored in this
    /// `Registry`.
    ///
    /// If this method returns `false`, then the [`Registry::len`] method will
    /// return a value greater than zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::Registry;
    ///
    /// static REGISTRY: Registry<&'static str, 4> = Registry::new();
    ///
    /// // Initially, the registry is empty.
    /// assert!(REGISTRY.is_empty());
    ///
    /// // After registering an entry, the registry is no longer empty.
    /// REGISTRY.register("foo");
    /// assert!(!REGISTRY.is_empty());
    /// ```
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the _total_ number of entries that may be stored in this
    /// `Registry`. This number includes the current [length] of the `Registry`.
    ///
    /// The [`remaining_capacity`](Self::remaining_capacity) method returns the
    /// number of _additional_ entries that may be stored in this `Registry`,
    /// i.e. the capacity minus the current [length] of the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::Registry;
    ///
    /// static REGISTRY: Registry<&'static str, 4> = Registry::new();
    ///
    /// assert_eq!(REGISTRY.capacity(), 4);
    ///
    /// // Register a value.
    /// REGISTRY.register("foo");
    ///
    /// // `capacity` still returns the *total* capacity, including the existing
    /// // values.
    /// assert_eq!(REGISTRY.capacity(), 4);
    /// ```
    ///
    /// [length]: Self::len
    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Returns `true` if no more entries can be stored in this `Registry`.
    ///
    /// If this method returns 0, then subsequent calls to [`register`],
    /// [`try_register`], and [`try_register_default`] will fail.
    ///
    /// This can be calculated by subtracting the registry's [length](Self::len)
    /// from its [capacity](Self::capacity).
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::Registry;
    ///
    /// // Create a `Registry` with a capacity of four entries.
    /// static REGISTRY: Registry<&'static str, 4> = Registry::new();
    ///
    /// // Initially, the registry is empty.
    /// assert!(!REGISTRY.is_full());
    ///
    /// // After registering an entry, the registry's remaining capacity decreases
    /// // by one.
    /// REGISTRY.try_register("foo").expect("registry is not full");
    /// assert!(!REGISTRY.is_full());
    ///
    /// REGISTRY.try_register("bar").expect("registry is not full");
    /// assert!(!REGISTRY.is_full());
    ///
    /// REGISTRY.try_register("baz").expect("registry is not full");
    /// assert!(!REGISTRY.is_full());
    ///
    /// // After registering the fourth entry, the registry is full.
    /// REGISTRY.try_register("quux").expect("registry is not full");
    /// assert!(REGISTRY.is_full());
    ///
    /// // Once the registry is full, we can no longer register
    /// // new entries.
    /// assert_eq!(REGISTRY.try_register("womble"), Err("womble"));
    /// ```
    ///
    /// [`try_register`]: Self::try_register
    /// [`try_register_default`]: Self::try_register_default
    /// [`register`]: Self::register
    #[must_use]
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Returns the number of entries that can _still_ be stored in this
    /// `Registry`.
    ///
    /// If this method returns 0, then subsequent calls to [`register`],
    /// [`try_register`], and [`try_register_default`] will fail.
    ///
    /// This can be calculated by subtracting the registry's [length](Self::len)
    /// from its [capacity](Self::capacity).
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::Registry;
    ///
    /// // Create a `Registry` with a capacity of four entries.
    /// static REGISTRY: Registry<&'static str, 4> = Registry::new();
    ///
    /// // Initially, the registry is empty.
    /// assert_eq!(REGISTRY.remaining_capacity(), REGISTRY.capacity());
    /// assert_eq!(REGISTRY.remaining_capacity(), 4);
    ///
    /// // After registering an entry, the registry's remaining capacity decreases
    /// // by one.
    /// REGISTRY.try_register("foo").expect("registry is not full");
    /// assert_ne!(REGISTRY.remaining_capacity(), REGISTRY.capacity());
    /// assert_eq!(REGISTRY.remaining_capacity(), 3);
    ///
    /// REGISTRY.try_register("bar").expect("registry is not full");
    /// assert_eq!(REGISTRY.remaining_capacity(), 2);
    ///
    /// REGISTRY.try_register("baz").expect("registry is not full");
    /// assert_eq!(REGISTRY.remaining_capacity(), 1);
    ///
    /// // After registering the fourth entry, the remaining capacity is zero.
    /// REGISTRY.try_register("quux").expect("registry is not full");
    /// assert_eq!(REGISTRY.remaining_capacity(), 0);
    ///
    /// // Once `remaining_capacity` returns zero, we can no longer register
    /// // new entries.
    /// assert_eq!(REGISTRY.try_register("womble"), Err("womble"));
    /// ```
    ///
    /// [`try_register`]: Self::try_register
    /// [`try_register_default`]: Self::try_register_default
    /// [`register`]: Self::register
    #[must_use]
    #[inline]
    pub fn remaining_capacity(&self) -> usize {
        self.capacity() - self.len()
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
    /// Returns a new `RegistryMap` with space for up to `CAPACITY` key-value
    /// pairs.
    #[must_use]
    pub const fn new() -> Self {
        Self(Registry::new())
    }

    /// Returns the value associated with the given `key`, or registers the
    /// value returned by `init` with that key if no value exists.
    ///
    /// This is an O(_n_) operation, where _n_ is the current
    /// [length](Self::len) of this `RegistryMap`.
    ///
    /// # Returns
    ///
    /// A reference to the value associated with `key`, or `None` if this
    /// `RegistryMap` is [full](Self::is_full).
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Registering a new entry.
    /// let value1 = REGISTRY.get_or_register_with("answer", || 42).unwrap();
    /// assert_eq!(*value1, 42);
    ///
    /// // Subsequent calls to `get_or_register_with` with the same key will return
    /// // a reference to the previously registered entry.
    /// let value2 = REGISTRY.get_or_register_with("answer", || 0).unwrap();
    /// assert_eq!(*value2, 42);
    ///
    /// // `value2` is a reference to the same entry as `value1`.
    /// assert!(core::ptr::eq(value1, value2));
    /// ```
    pub fn get_or_register_with(&self, key: K, init: impl FnOnce() -> V) -> Option<&V>
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

    /// Returns the value associated with the given `key`, or registers the
    /// value returned by `V::default()` if no value exists for this `key`.
    ///
    /// This function is equivalent to calling [`get_or_register_with`] and
    /// passing [`Default::default`] as the `init` function:
    /// ```rust
    /// # let key = "foo";
    /// # let registry = tinymetrics::registry::RegistryMap::<&'static str, usize, 4>::new();
    /// registry.get_or_register_with(key, Default::default);
    /// ```
    ///
    /// This is an O(_n_) operation, where _n_ is the current
    /// [length](Self::len) of this `RegistryMap`.
    ///
    /// # Returns
    ///
    /// A reference to the value associated with `key`, or `None` if this
    /// `RegistryMap` is [full](Self::is_full).
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Registering a new entry.
    /// let value1 = REGISTRY.get_or_register_default("foo").unwrap();
    /// // The new entry is initialized to the default value of `usize`.
    /// assert_eq!(*value1, 0);
    ///
    /// // Subsequent attempts to register the same key will return
    /// // a reference to the previously registered entry.
    /// let value2 = REGISTRY.get_or_register("foo", 1).unwrap();
    /// assert_eq!(*value2, 0);
    ///
    /// // `value2` is a reference to the same entry as `value1`.
    /// assert!(core::ptr::eq(value1, value2));
    /// ```
    ///
    /// [`get_or_register_with`]: Self::get_or_register_with
    pub fn get_or_register_default(&self, key: K) -> Option<&V>
    where
        K: PartialEq,
        V: Default,
    {
        self.get_or_register_with(key, V::default)
    }

    /// Returns the value associated with the given `key`, or registers `value`
    /// associated with that key if no value exists.
    ///
    /// This is an O(_n_) operation, where _n_ is the current
    /// [length](Self::len) of this `RegistryMap`.
    ///
    /// # Returns
    ///
    /// A reference to the value associated with `key`, or `None` if this
    /// `RegistryMap` is [full](Self::is_full).
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Registering a new value
    /// let value1 = REGISTRY.get_or_register("answer", 42).unwrap();
    /// assert_eq!(*value1, 42);
    ///
    /// // Subsequent calls to `get_or_register` with the same key will return
    /// // a reference to the previously registered value.
    /// let value2 = REGISTRY.get_or_register("answer", 0).unwrap();
    /// assert_eq!(*value2, 42);
    ///
    /// // `value2` is a reference to the same entry as `value1`.
    /// assert!(core::ptr::eq(value1, value2));
    /// ```
    pub fn get_or_register(&self, key: K, value: V) -> Option<&V>
    where
        K: PartialEq,
    {
        self.get_or_register_with(key, move || value)
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

    /// Returns the number of key-value pairs _currently_ stored in this
    /// `RegistryMap`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Initially, the length is 0:
    /// assert_eq!(REGISTRY.len(), 0);
    ///
    /// // Registering a new entry increases the length by 1:
    /// REGISTRY.get_or_register_default("foo");
    /// assert_eq!(REGISTRY.len(), 1);
    /// ```
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if _no_ key-value pairs are currently stored in this
    /// `RegistryMap`.
    ///
    /// If this method returns `true`, then the [`RegistryMap::len`] method will
    /// return a value greater than zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Initially, the map is empty.
    /// assert!(REGISTRY.is_empty());
    ///
    /// // After registering an entry, the map is no longer empty.
    /// REGISTRY.get_or_register_default("foo");
    /// assert!(!REGISTRY.is_empty());
    /// ```
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the _total_ number of entries that may be stored in this
    /// `RegistryMap`. This number includes the current [length] of the map.
    ///
    /// The [`remaining_capacity`](Self::remaining_capacity) method returns the
    /// number of _additional_ entries that may be stored in this `RegistryMap`,
    /// i.e. the capacity minus the current [length] of the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// assert_eq!(REGISTRY.capacity(), 4);
    ///
    /// // Register a value.
    /// REGISTRY.get_or_register_default("foo").unwrap();
    ///
    /// // `capacity` still returns the *total* capacity, including the existing
    /// // values.
    /// assert_eq!(REGISTRY.capacity(), 4);
    /// ```
    ///
    /// [length]: Self::len
    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Returns `true` if no more entries can be stored in this `RegistryMap`.
    ///
    /// If this method returns `true`, then subsequent calls to
    /// [`get_or_register`], [`get_or_register_with`], and
    /// [`get_or_register_default`] will return [`None`].
    ///
    /// The number of entries that may be stored in this `RegistryMap` before it
    /// becomes full can be checked by calling the [`remaining_capacity`] method.
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// // Create a `RegistryMap` with a capacity of four entries.
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Initially, the map is empty.
    /// assert!(!REGISTRY.is_full());
    ///
    /// // After registering an entry, the map is no longer empty.
    /// REGISTRY.get_or_register_default("foo").expect("registry is not full");
    /// assert!(!REGISTRY.is_full());
    ///
    /// REGISTRY.get_or_register_default("bar").expect("registry is not full");
    /// assert!(!REGISTRY.is_full());
    ///
    /// REGISTRY.get_or_register_default("baz").expect("registry is not full");
    /// assert!(!REGISTRY.is_full());
    ///
    /// // After registering the fourth entry, the map is now full.
    /// REGISTRY.get_or_register_default("quux").expect("registry is not full");
    /// assert!(REGISTRY.is_full());
    ///
    /// // Now that the map is full, we can no longer register new entries:
    /// assert_eq!(REGISTRY.get_or_register_default("womble"), None);
    /// ```
    ///
    /// [`get_or_register`]: Self::get_or_register
    /// [`get_or_register_with`]: Self::get_or_register_with
    /// [`get_or_register_default`]: Self::get_or_register_default
    /// [`remaining_capacity]: Self::remaining_capacity
    #[must_use]
    #[inline]
    pub fn is_full(&self) -> bool {
        self.0.is_full()
    }

    /// Returns the number of entries that can still be stored in this
    /// `RegistryMap`.
    ///
    /// If this method returns 0, then subsequent calls to
    /// [`get_or_register`], [`get_or_register_with`], and
    /// [`get_or_register_default`] will return [`None`].
    ///
    /// This can be calculated by subtracting the registry's [length](Self::len)
    /// from its [capacity](Self::capacity).
    ///
    /// # Examples
    ///
    /// ```
    /// use tinymetrics::registry::RegistryMap;
    ///
    /// // Create a `RegistryMap` with a capacity of four entries.
    /// static REGISTRY: RegistryMap<&'static str, usize, 4> = RegistryMap::new();
    ///
    /// // Initially, the map is empty.
    /// assert_eq!(REGISTRY.remaining_capacity(), REGISTRY.capacity());
    /// assert_eq!(REGISTRY.remaining_capacity(), 4);
    ///
    /// // After registering an entry, the map's remaining capacity decreases
    /// // by one.
    /// REGISTRY.get_or_register_default("foo").expect("registry is not full");
    /// assert_ne!(REGISTRY.remaining_capacity(), REGISTRY.capacity());
    /// assert_eq!(REGISTRY.remaining_capacity(), 3);
    ///
    /// REGISTRY.get_or_register_default("bar").expect("registry is not full");
    /// assert_eq!(REGISTRY.remaining_capacity(), 2);
    ///
    /// REGISTRY.get_or_register_default("baz").expect("registry is not full");
    /// assert_eq!(REGISTRY.remaining_capacity(), 1);
    ///
    /// // After registering the fourth entry, the remaining capacity is zero.
    /// REGISTRY.get_or_register_default("quux").expect("registry is not full");
    /// assert_eq!(REGISTRY.remaining_capacity(), 0);
    ///
    /// // Once `remaining_capacity` returns zero, we can no longer register
    /// // new entries in the map.
    /// assert_eq!(REGISTRY.get_or_register_default("womble"), None);
    /// ```
    ///
    /// [`get_or_register`]: Self::get_or_register
    /// [`get_or_register_with`]: Self::get_or_register_with
    /// [`get_or_register_default`]: Self::get_or_register_default
    #[must_use]
    #[inline]
    pub fn remaining_capacity(&self) -> usize {
        self.0.remaining_capacity()
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
