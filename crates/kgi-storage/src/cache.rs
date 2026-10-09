use kgi_model::block::{BlockCoordinate, BlockHash, CompactId};
use moka::sync::Cache;

const IDENTITY_CACHE_CAPACITY: u64 = 432_000;
const COORDINATE_CACHE_CAPACITY: u64 = 432_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CachedIdentity {
    pub(crate) id: CompactId,
    pub(crate) materialized: bool,
}

pub(crate) struct ProcessingCaches {
    identities: Cache<BlockHash, CachedIdentity>,
    coordinates: Cache<CompactId, BlockCoordinate>,
}

impl ProcessingCaches {
    pub(crate) fn new() -> Self {
        Self::with_capacities(IDENTITY_CACHE_CAPACITY, COORDINATE_CACHE_CAPACITY)
    }

    fn with_capacities(identity_capacity: u64, coordinate_capacity: u64) -> Self {
        Self { identities: Cache::new(identity_capacity), coordinates: Cache::new(coordinate_capacity) }
    }

    pub(crate) fn identity(&self, hash: BlockHash) -> Option<CachedIdentity> {
        self.identities.get(&hash)
    }

    pub(crate) fn coordinate(&self, id: CompactId) -> Option<BlockCoordinate> {
        self.coordinates.get(&id)
    }

    pub(crate) fn publish(&self, hash: BlockHash, identity: CachedIdentity, coordinate: Option<BlockCoordinate>) {
        self.identities.insert(hash, identity);
        if let Some(coordinate) = coordinate {
            self.coordinates.insert(identity.id, coordinate);
        }
    }
}

#[cfg(test)]
mod tests {
    use kgi_model::block::{BlockCoordinate, BlockHash, CompactId};

    use super::{CachedIdentity, ProcessingCaches};

    fn hash(byte: u8) -> BlockHash {
        BlockHash::from_bytes([byte; 32])
    }

    fn id(value: i64) -> CompactId {
        CompactId::new(value).expect("positive compact ID")
    }

    #[test]
    fn default_capacities_cover_twelve_hours_at_ten_blocks_per_second() {
        let caches = ProcessingCaches::new();

        assert_eq!(caches.identities.policy().max_capacity(), Some(432_000));
        assert_eq!(caches.coordinates.policy().max_capacity(), Some(432_000));
    }

    #[test]
    fn caches_have_independent_capacity_policies() {
        let caches = ProcessingCaches::with_capacities(2, 2);
        let coordinate = |level| BlockCoordinate::new(level, 0).expect("positive level");

        caches.publish(hash(1), CachedIdentity { id: id(1), materialized: true }, Some(coordinate(1)));
        caches.publish(hash(2), CachedIdentity { id: id(2), materialized: true }, Some(coordinate(2)));

        assert_eq!(caches.identities.policy().max_capacity(), Some(2));
        assert_eq!(caches.coordinates.policy().max_capacity(), Some(2));
        assert_eq!(caches.identity(hash(1)).map(|identity| identity.id), Some(id(1)));
        assert_eq!(caches.coordinate(id(1)), Some(coordinate(1)));

        caches.publish(hash(3), CachedIdentity { id: id(3), materialized: true }, Some(coordinate(3)));
        caches.identities.run_pending_tasks();
        caches.coordinates.run_pending_tasks();

        assert!(caches.identities.entry_count() <= 2);
        assert!(caches.coordinates.entry_count() <= 2);
    }
}
