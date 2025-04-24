plait::plait! {
    with crates {
        merde
        rusqlite
    }

    /// The key of an object in the object store
    pub struct ObjectStoreKey => &ObjectStoreKeyRef;
}
