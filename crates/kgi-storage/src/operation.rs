use std::sync::Arc;

use kgi_model::{
    block::{BlockCoordinate, BlockHash, CompactId},
    graph_update::BlockCommitted,
};
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

/// Materiality required for references consumed by block materialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferencePolicy {
    /// Missing references may become permanent outside-boundary identities.
    AllowBoundaryIdentities,
    /// Every reference must already be materialized.
    RequireMaterialized,
}

/// Successful result of one authoritative block-materiality operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializeBlockOutcome {
    /// The block and its complete graph-update projection were committed.
    Inserted { committed: BlockCommitted },
    /// The block was already materialized and no graph mutation occurred.
    AlreadyMaterialized { id: CompactId, coordinate: BlockCoordinate },
}

/// Failure to materialize one validated block.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MaterializeBlockError {
    /// The storage operation itself failed.
    #[error(transparent)]
    Storage(#[from] StorageError),

    /// The incoming block was previously fixed as a permanent boundary identity.
    #[error("incoming block is already a permanent boundary identity: {hash}")]
    IncomingBoundaryIdentity { hash: BlockHash },

    /// At least one required reference is not materialized.
    #[error("block references include absent or boundary-only identities")]
    NonMaterializedReferences {
        /// Absent references, deduplicated in universal first-occurrence order.
        missing: Arc<[BlockHash]>,
        /// Boundary references, deduplicated in universal first-occurrence order.
        identity_only: Arc<[BlockHash]>,
    },
}
