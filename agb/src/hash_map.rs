use alloc::vec::Vec;
use core::{
    hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
    iter::{self, FromIterator},
    mem::{self, MaybeUninit},
    ops::Index,
    ptr,
};

use rustc_hash::FxHasher;

type HashType = u32;

pub struct HashMap<K, V, H = BuildHasherDefault<FxHasher>>
where
    H: BuildHasher,
{
    nodes: NodeStorage<K, V>,

    hasher: H,
}

impl<K, V> HashMap<K, V> {
    pub fn new() -> Self {
        Self::with_capacity(16)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: NodeStorage::with_size(capacity),
            hasher: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn resize(&mut self, new_size: usize) {
        assert!(
            new_size >= self.nodes.capacity(),
            "Can only increase the size of a hash map"
        );
        if new_size == self.nodes.capacity() {
            return;
        }

        self.nodes = self.nodes.resized_to(new_size);
    }
}

impl<K, V> Default for HashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

const fn fast_mod(len: usize, hash: HashType) -> usize {
    debug_assert!(len.is_power_of_two(), "Length must be a power of 2");
    (hash as usize) & (len - 1)
}

impl<K, V> HashMap<K, V>
where
    K: Eq + Hash,
{
    pub fn insert(&mut self, key: K, value: V) -> &mut V {
        let hash = self.hash(&key);

        let location = if let Some(location) = self.nodes.get_location(&key, hash) {
            self.nodes.replace_at_location(location, key, value);
            location
        } else {
            if self.nodes.capacity() * 85 / 100 <= self.len() {
                self.resize(self.nodes.capacity() * 2);
            }

            self.nodes.insert_new(key, value, hash)
        };

        self.nodes.nodes[location].value_mut().unwrap()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let hash = self.hash(key);

        self.nodes
            .get_location(key, hash)
            .and_then(|location| self.nodes.nodes[location].value_ref())
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let hash = self.hash(key);

        if let Some(location) = self.nodes.get_location(key, hash) {
            self.nodes.nodes[location].value_mut()
        } else {
            None
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let hash = self.hash(key);

        self.nodes
            .get_location(key, hash)
            .map(|location| self.nodes.remove_from_location(location))
    }
}

impl<K, V> HashMap<K, V>
where
    K: Hash,
{
    fn hash(&self, key: &K) -> HashType {
        let mut hasher = self.hasher.build_hasher();
        key.hash(&mut hasher);
        hasher.finish() as HashType
    }
}

pub struct Iter<'a, K: 'a, V: 'a> {
    map: &'a HashMap<K, V>,
    at: usize,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.at >= self.map.nodes.capacity() {
                return None;
            }

            let node = &self.map.nodes.nodes[self.at];
            self.at += 1;

            if node.has_value() {
                return Some((node.key_ref().unwrap(), node.value_ref().unwrap()));
            }
        }
    }
}

impl<'a, K, V> IntoIterator for &'a HashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter { map: self, at: 0 }
    }
}

pub struct OccupiedEntry<'a, K: 'a, V: 'a> {
    key: K,
    map: &'a mut HashMap<K, V>,
    location: usize,
}

impl<'a, K: 'a, V: 'a> OccupiedEntry<'a, K, V> {
    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn remove_entry(self) -> (K, V) {
        let old_value = self.map.nodes.remove_from_location(self.location);
        (self.key, old_value)
    }

    pub fn get(&self) -> &V {
        self.map.nodes.nodes[self.location].value_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut V {
        self.map.nodes.nodes[self.location].value_mut().unwrap()
    }

    pub fn into_mut(self) -> &'a mut V {
        self.map.nodes.nodes[self.location].value_mut().unwrap()
    }

    pub fn insert(&mut self, value: V) -> V {
        self.map.nodes.nodes[self.location].replace_value(value)
    }

    pub fn remove(self) -> V {
        self.map.nodes.remove_from_location(self.location)
    }
}

pub struct VacantEntry<'a, K: 'a, V: 'a> {
    key: K,
    map: &'a mut HashMap<K, V>,
}

impl<'a, K: 'a, V: 'a> VacantEntry<'a, K, V> {
    pub fn key(&self) -> &K {
        &self.key
    }

    pub fn into_key(self) -> K {
        self.key
    }

    pub fn insert(self, value: V) -> &'a mut V
    where
        K: Hash + Eq,
    {
        self.map.insert(self.key, value)
    }
}

pub enum Entry<'a, K: 'a, V: 'a> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V>
where
    K: Hash + Eq,
{
    pub fn or_insert(self, value: V) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(value),
        }
    }

    pub fn or_insert_with<F>(self, f: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(f()),
        }
    }

    pub fn or_insert_with_key<F>(self, f: F) -> &'a mut V
    where
        F: FnOnce(&K) -> V,
    {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let value = f(&e.key);
                e.insert(value)
            }
        }
    }

    pub fn and_modify<F>(self, f: F) -> Self
    where
        F: FnOnce(&mut V),
    {
        match self {
            Entry::Occupied(mut e) => {
                f(e.get_mut());
                Entry::Occupied(e)
            }
            Entry::Vacant(e) => Entry::Vacant(e),
        }
    }

    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(Default::default()),
        }
    }

    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(e) => &e.key,
            Entry::Vacant(e) => &e.key,
        }
    }
}

impl<'a, K, V> HashMap<K, V>
where
    K: Hash + Eq,
{
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        let hash = self.hash(&key);
        let location = self.nodes.get_location(&key, hash);

        if let Some(location) = location {
            Entry::Occupied(OccupiedEntry {
                key,
                location,
                map: self,
            })
        } else {
            Entry::Vacant(VacantEntry { key, map: self })
        }
    }
}

impl<K, V> FromIterator<(K, V)> for HashMap<K, V>
where
    K: Eq + Hash,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut map = HashMap::new();
        map.extend(iter);
        map
    }
}

impl<K, V> Extend<(K, V)> for HashMap<K, V>
where
    K: Eq + Hash,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<K, V> Index<&K> for HashMap<K, V>
where
    K: Eq + Hash,
{
    type Output = V;

    fn index(&self, key: &K) -> &V {
        self.get(key).expect("no entry found for key")
    }
}

impl<K, V> Index<K> for HashMap<K, V>
where
    K: Eq + Hash,
{
    type Output = V;

    fn index(&self, key: K) -> &V {
        self.get(&key).expect("no entry found for key")
    }
}

struct NodeStorage<K, V> {
    nodes: Vec<Node<K, V>>,
    max_distance_to_initial_bucket: i32,

    number_of_items: usize,
}

impl<K, V> NodeStorage<K, V> {
    fn with_size(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be a power of 2");

        Self {
            nodes: iter::repeat_with(Default::default).take(capacity).collect(),
            max_distance_to_initial_bucket: 0,
            number_of_items: 0,
        }
    }

    fn capacity(&self) -> usize {
        self.nodes.len()
    }

    fn len(&self) -> usize {
        self.number_of_items
    }

    fn insert_new(&mut self, key: K, value: V, hash: HashType) -> usize {
        debug_assert!(
            self.capacity() * 85 / 100 > self.len(),
            "Do not have space to insert into len {} with {}",
            self.capacity(),
            self.len()
        );

        let mut new_node = Node::new_with(key, value, hash);
        let mut inserted_location = usize::MAX;

        loop {
            let location = fast_mod(
                self.capacity(),
                new_node.hash + new_node.get_distance() as HashType,
            );
            let current_node = &mut self.nodes[location];

            if current_node.has_value() {
                if current_node.get_distance() <= new_node.get_distance() {
                    mem::swap(&mut new_node, current_node);

                    if inserted_location == usize::MAX {
                        inserted_location = location;
                    }
                }
            } else {
                self.nodes[location] = new_node;
                if inserted_location == usize::MAX {
                    inserted_location = location;
                }
                break;
            }

            new_node.increment_distance();
            self.max_distance_to_initial_bucket = new_node
                .get_distance()
                .max(self.max_distance_to_initial_bucket);
        }

        self.number_of_items += 1;
        inserted_location
    }

    fn remove_from_location(&mut self, location: usize) -> V {
        let mut current_location = location;
        self.number_of_items -= 1;

        loop {
            let next_location = fast_mod(self.capacity(), (current_location + 1) as HashType);

            // if the next node is empty, or the next location has 0 distance to initial bucket then
            // we can clear the current node
            if !self.nodes[next_location].has_value()
                || self.nodes[next_location].get_distance() == 0
            {
                return self.nodes[current_location].take_key_value().unwrap().1;
            }

            self.nodes.swap(current_location, next_location);
            self.nodes[current_location].decrement_distance();
            current_location = next_location;
        }
    }

    fn get_location(&self, key: &K, hash: HashType) -> Option<usize>
    where
        K: Eq,
    {
        for distance_to_initial_bucket in 0..=self.max_distance_to_initial_bucket {
            let location = fast_mod(
                self.nodes.len(),
                hash + distance_to_initial_bucket as HashType,
            );

            let node = &self.nodes[location];
            if let Some(node_key_ref) = node.key_ref() {
                if node_key_ref == key {
                    return Some(location);
                }
            } else {
                return None;
            }
        }

        None
    }

    fn resized_to(&mut self, new_size: usize) -> Self {
        let mut new_node_storage = Self::with_size(new_size);

        for mut node in self.nodes.drain(..) {
            if let Some((key, value, hash)) = node.take_key_value() {
                new_node_storage.insert_new(key, value, hash);
            }
        }

        new_node_storage
    }

    fn replace_at_location(&mut self, location: usize, key: K, value: V) -> V {
        self.nodes[location].replace(key, value).1
    }
}

struct Node<K, V> {
    hash: HashType,

    // distance_to_initial_bucket = -1 => key and value are uninit.
    // distance_to_initial_bucket >= 0 => key and value are init
    distance_to_initial_bucket: i32,
    key: MaybeUninit<K>,
    value: MaybeUninit<V>,
}

impl<K, V> Node<K, V> {
    fn new() -> Self {
        Self {
            hash: 0,
            distance_to_initial_bucket: -1,
            key: MaybeUninit::uninit(),
            value: MaybeUninit::uninit(),
        }
    }

    fn new_with(key: K, value: V, hash: HashType) -> Self {
        Self {
            hash,
            distance_to_initial_bucket: 0,
            key: MaybeUninit::new(key),
            value: MaybeUninit::new(value),
        }
    }

    fn value_ref(&self) -> Option<&V> {
        if self.has_value() {
            Some(unsafe { self.value.assume_init_ref() })
        } else {
            None
        }
    }

    fn value_mut(&mut self) -> Option<&mut V> {
        if self.has_value() {
            Some(unsafe { self.value.assume_init_mut() })
        } else {
            None
        }
    }

    fn key_ref(&self) -> Option<&K> {
        if self.distance_to_initial_bucket >= 0 {
            Some(unsafe { self.key.assume_init_ref() })
        } else {
            None
        }
    }

    fn has_value(&self) -> bool {
        self.distance_to_initial_bucket >= 0
    }

    fn take_key_value(&mut self) -> Option<(K, V, HashType)> {
        if self.has_value() {
            let key = mem::replace(&mut self.key, MaybeUninit::uninit());
            let value = mem::replace(&mut self.value, MaybeUninit::uninit());
            self.distance_to_initial_bucket = -1;

            Some(unsafe { (key.assume_init(), value.assume_init(), self.hash) })
        } else {
            None
        }
    }

    fn replace_value(&mut self, value: V) -> V {
        if self.has_value() {
            let old_value = mem::replace(&mut self.value, MaybeUninit::new(value));
            unsafe { old_value.assume_init() }
        } else {
            panic!("Cannot replace an unininitalised node");
        }
    }

    fn replace(&mut self, key: K, value: V) -> (K, V) {
        if self.has_value() {
            let old_key = mem::replace(&mut self.key, MaybeUninit::new(key));
            let old_value = mem::replace(&mut self.value, MaybeUninit::new(value));

            unsafe { (old_key.assume_init(), old_value.assume_init()) }
        } else {
            panic!("Cannot replace an uninitialised node");
        }
    }

    fn increment_distance(&mut self) {
        self.distance_to_initial_bucket += 1;
    }

    fn decrement_distance(&mut self) {
        self.distance_to_initial_bucket -= 1;
        if self.distance_to_initial_bucket < 0 {
            panic!("Cannot decrement distance to below 0");
        }
    }

    fn get_distance(&self) -> i32 {
        self.distance_to_initial_bucket
    }
}

impl<K, V> Drop for Node<K, V> {
    fn drop(&mut self) {
        if self.has_value() {
            unsafe { ptr::drop_in_place(self.key.as_mut_ptr()) };
            unsafe { ptr::drop_in_place(self.value.as_mut_ptr()) };
        }
    }
}

impl<K, V> Default for Node<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use core::cell::RefCell;

    use super::*;
    use crate::Gba;

    #[test_case]
    fn can_store_and_retrieve_8_elements(_gba: &mut Gba) {
        let mut map = HashMap::new();

        for i in 0..8 {
            map.insert(i, i % 4);
        }

        for i in 0..8 {
            assert_eq!(map.get(&i), Some(&(i % 4)));
        }
    }

    #[test_case]
    fn can_get_the_length(_gba: &mut Gba) {
        let mut map = HashMap::new();

        for i in 0..8 {
            map.insert(i / 2, true);
        }

        assert_eq!(map.len(), 4);
    }

    #[test_case]
    fn returns_none_if_element_does_not_exist(_gba: &mut Gba) {
        let mut map = HashMap::new();

        for i in 0..8 {
            map.insert(i, i % 3);
        }

        assert_eq!(map.get(&12), None);
    }

    #[test_case]
    fn can_delete_entries(_gba: &mut Gba) {
        let mut map = HashMap::new();

        for i in 0..8 {
            map.insert(i, i % 3);
        }

        for i in 0..4 {
            map.remove(&i);
        }

        assert_eq!(map.len(), 4);
        assert_eq!(map.get(&3), None);
        assert_eq!(map.get(&7), Some(&1));
    }

    #[test_case]
    fn can_iterate_through_all_entries(_gba: &mut Gba) {
        let mut map = HashMap::new();

        for i in 0..8 {
            map.insert(i, i);
        }

        let mut max_found = -1;
        let mut num_found = 0;

        for (_, value) in map.into_iter() {
            max_found = max_found.max(*value);
            num_found += 1;
        }

        assert_eq!(num_found, 8);
        assert_eq!(max_found, 7);
    }

    #[test_case]
    fn can_insert_more_than_initial_capacity(_gba: &mut Gba) {
        let mut map = HashMap::new();

        for i in 0..65 {
            map.insert(i, i % 4);
        }

        for i in 0..65 {
            assert_eq!(map.get(&i), Some(&(i % 4)));
        }
    }

    struct RandomNumberGenerator {
        state: [u32; 4],
    }

    impl RandomNumberGenerator {
        const fn new() -> Self {
            Self {
                state: [1014776995, 476057059, 3301633994, 706340607],
            }
        }

        fn next(&mut self) -> i32 {
            let result = (self.state[0].wrapping_add(self.state[3]))
                .rotate_left(7)
                .wrapping_mul(9);
            let t = self.state[1].wrapping_shr(9);

            self.state[2] ^= self.state[0];
            self.state[3] ^= self.state[1];
            self.state[1] ^= self.state[2];
            self.state[0] ^= self.state[3];

            self.state[2] ^= t;
            self.state[3] = self.state[3].rotate_left(11);

            result as i32
        }
    }

    struct NoisyDrop {
        i: i32,
        dropped: bool,
    }

    impl NoisyDrop {
        fn new(i: i32) -> Self {
            Self { i, dropped: false }
        }
    }

    impl PartialEq for NoisyDrop {
        fn eq(&self, other: &Self) -> bool {
            self.i == other.i
        }
    }

    impl Eq for NoisyDrop {}

    impl Hash for NoisyDrop {
        fn hash<H: Hasher>(&self, hasher: &mut H) {
            hasher.write_i32(self.i);
        }
    }

    impl Drop for NoisyDrop {
        fn drop(&mut self) {
            if self.dropped {
                panic!("NoisyDropped dropped twice");
            }

            self.dropped = true;
        }
    }

    #[test_case]
    fn extreme_case(_gba: &mut Gba) {
        let mut map = HashMap::new();
        let mut rng = RandomNumberGenerator::new();

        let mut answers: [Option<i32>; 128] = [None; 128];

        for _ in 0..5_000 {
            let command = rng.next().rem_euclid(2);
            let key = rng.next().rem_euclid(answers.len() as i32);
            let value = rng.next();

            match command {
                0 => {
                    // insert
                    answers[key as usize] = Some(value);
                    map.insert(NoisyDrop::new(key), NoisyDrop::new(value));
                }
                1 => {
                    // remove
                    answers[key as usize] = None;
                    map.remove(&NoisyDrop::new(key));
                }
                _ => {}
            }

            for (i, answer) in answers.iter().enumerate() {
                assert_eq!(
                    map.get(&NoisyDrop::new(i as i32)).map(|nd| &nd.i),
                    answer.as_ref()
                );
            }
        }
    }

    #[derive(Clone)]
    struct Droppable<'a> {
        id: usize,
        drop_registry: &'a DropRegistry,
    }

    impl Hash for Droppable<'_> {
        fn hash<H: Hasher>(&self, hasher: &mut H) {
            hasher.write_usize(self.id);
        }
    }

    impl PartialEq for Droppable<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }

    impl Eq for Droppable<'_> {}

    impl Drop for Droppable<'_> {
        fn drop(&mut self) {
            self.drop_registry.dropped(self.id);
        }
    }

    struct DropRegistry {
        are_dropped: RefCell<Vec<i32>>,
    }

    impl DropRegistry {
        pub fn new() -> Self {
            Self {
                are_dropped: Default::default(),
            }
        }

        pub fn new_droppable(&self) -> Droppable<'_> {
            self.are_dropped.borrow_mut().push(0);
            Droppable {
                id: self.are_dropped.borrow().len() - 1,
                drop_registry: self,
            }
        }

        pub fn dropped(&self, id: usize) {
            self.are_dropped.borrow_mut()[id] += 1;
        }

        pub fn assert_dropped_once(&self, id: usize) {
            assert_eq!(self.are_dropped.borrow()[id], 1);
        }

        pub fn assert_not_dropped(&self, id: usize) {
            assert_eq!(self.are_dropped.borrow()[id], 0);
        }

        pub fn assert_dropped_n_times(&self, id: usize, num_drops: i32) {
            assert_eq!(self.are_dropped.borrow()[id], num_drops);
        }
    }

    #[test_case]
    fn correctly_drops_on_remove_and_overall_drop(_gba: &mut Gba) {
        let drop_registry = DropRegistry::new();

        let droppable1 = drop_registry.new_droppable();
        let droppable2 = drop_registry.new_droppable();

        let id1 = droppable1.id;
        let id2 = droppable2.id;

        {
            let mut map = HashMap::new();

            map.insert(1, droppable1);
            map.insert(2, droppable2);

            drop_registry.assert_not_dropped(id1);
            drop_registry.assert_not_dropped(id2);

            map.remove(&1);
            drop_registry.assert_dropped_once(id1);
            drop_registry.assert_not_dropped(id2);
        }

        drop_registry.assert_dropped_once(id2);
    }

    #[test_case]
    fn correctly_drop_on_override(_gba: &mut Gba) {
        let drop_registry = DropRegistry::new();

        let droppable1 = drop_registry.new_droppable();
        let droppable2 = drop_registry.new_droppable();

        let id1 = droppable1.id;
        let id2 = droppable2.id;

        {
            let mut map = HashMap::new();

            map.insert(1, droppable1);
            drop_registry.assert_not_dropped(id1);
            map.insert(1, droppable2);

            drop_registry.assert_dropped_once(id1);
            drop_registry.assert_not_dropped(id2);
        }

        drop_registry.assert_dropped_once(id2);
    }

    #[test_case]
    fn correctly_drops_key_on_override(_gba: &mut Gba) {
        let drop_registry = DropRegistry::new();

        let droppable1 = drop_registry.new_droppable();
        let droppable1a = droppable1.clone();

        let id1 = droppable1.id;

        {
            let mut map = HashMap::new();

            map.insert(droppable1, 1);
            drop_registry.assert_not_dropped(id1);
            map.insert(droppable1a, 2);

            drop_registry.assert_dropped_once(id1);
        }

        drop_registry.assert_dropped_n_times(id1, 2);
    }

    // Following test cases copied from the rust source
    // https://github.com/rust-lang/rust/blob/master/library/std/src/collections/hash/map/tests.rs
    mod rust_std_tests {
        use crate::{
            hash_map::{Entry::*, HashMap},
            Gba,
        };

        #[test_case]
        fn test_entry(_gba: &mut Gba) {
            let xs = [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (6, 60)];

            let mut map: HashMap<_, _> = xs.iter().cloned().collect();

            // Existing key (insert)
            match map.entry(1) {
                Vacant(_) => unreachable!(),
                Occupied(mut view) => {
                    assert_eq!(view.get(), &10);
                    assert_eq!(view.insert(100), 10);
                }
            }
            assert_eq!(map.get(&1).unwrap(), &100);
            assert_eq!(map.len(), 6);

            // Existing key (update)
            match map.entry(2) {
                Vacant(_) => unreachable!(),
                Occupied(mut view) => {
                    let v = view.get_mut();
                    let new_v = (*v) * 10;
                    *v = new_v;
                }
            }
            assert_eq!(map.get(&2).unwrap(), &200);
            assert_eq!(map.len(), 6);

            // Existing key (take)
            match map.entry(3) {
                Vacant(_) => unreachable!(),
                Occupied(view) => {
                    assert_eq!(view.remove(), 30);
                }
            }
            assert_eq!(map.get(&3), None);
            assert_eq!(map.len(), 5);

            // Inexistent key (insert)
            match map.entry(10) {
                Occupied(_) => unreachable!(),
                Vacant(view) => {
                    assert_eq!(*view.insert(1000), 1000);
                }
            }
            assert_eq!(map.get(&10).unwrap(), &1000);
            assert_eq!(map.len(), 6);
        }

        #[test_case]
        fn test_occupied_entry_key(_gba: &mut Gba) {
            let mut a = HashMap::new();
            let key = "hello there";
            let value = "value goes here";
            assert!(a.is_empty());
            a.insert(key, value);
            assert_eq!(a.len(), 1);
            assert_eq!(a[key], value);

            match a.entry(key) {
                Vacant(_) => panic!(),
                Occupied(e) => assert_eq!(key, *e.key()),
            }
            assert_eq!(a.len(), 1);
            assert_eq!(a[key], value);
        }

        #[test_case]
        fn test_vacant_entry_key(_gba: &mut Gba) {
            let mut a = HashMap::new();
            let key = "hello there";
            let value = "value goes here";

            assert!(a.is_empty());
            match a.entry(key) {
                Occupied(_) => panic!(),
                Vacant(e) => {
                    assert_eq!(key, *e.key());
                    e.insert(value);
                }
            }
            assert_eq!(a.len(), 1);
            assert_eq!(a[key], value);
        }

        #[test_case]
        fn test_index(_gba: &mut Gba) {
            let mut map = HashMap::new();

            map.insert(1, 2);
            map.insert(2, 1);
            map.insert(3, 4);

            assert_eq!(map[&2], 1);
        }
    }
}