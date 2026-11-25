//! Shared Rust system contracts for Linos editions and recovery tools.

pub mod platform {
    pub mod edition;
}

pub mod recovery {
    pub mod mounts;
    pub mod records;
    pub mod update;
}
