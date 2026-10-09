use std::sync::Arc;

use kgi_model::block::BlockHash;
use thiserror::Error;

use crate::error::StorageError;

/// Failure to resolve an ordered batch exclusively to materialized block IDs.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ResolveMaterializedIdsError {
    /// The storage operation itself failed.
    #[error(transparent)]
    Storage(#[from] StorageError),

    /// At least one requested hash does not name a materialized block.
    #[error("requested hashes include absent or boundary-only block identities")]
    NonMaterialized {
        /// Absent hashes, deduplicated in first-occurrence order.
        missing: Arc<[BlockHash]>,
        /// Boundary identities, deduplicated in first-occurrence order.
        identity_only: Arc<[BlockHash]>,
    },
}
