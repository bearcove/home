plait::plait! {
    with crates {
        rusqlite
    }

    /// The key of an object in the object store
    pub struct ObjectStoreKey => &ObjectStoreKeyRef;
}
