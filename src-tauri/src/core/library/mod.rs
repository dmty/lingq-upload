pub mod index;
pub mod reconcile;

pub use index::{
    load_or_rebuild, rebuild_from_store, write_atomic, LibraryEntry, LibraryError, LibraryIndex,
    INDEX_FILENAME, INDEX_SCHEMA_V1,
};
pub use reconcile::{candidate_to_id, reconcile, ReconcileReport};
