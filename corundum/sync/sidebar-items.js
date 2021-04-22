initSidebarItems({"fn":[["init_lock",""]],"struct":[["MutexGuard",""],["PMutex","A transaction-wide recursive mutual exclusion primitive useful for protecting shared data while transaction is open. Further locking in the same thread is non-blocking. Any access to data is serialized. Borrow rules are checked dynamically to prevent multiple mutable dereferencing."],["Parc","A thread-safe reference-counting persistent pointer. ‘Parc’ stands for ‘Persistent Atomically Reference Counted’."],["ParcInner","The [`Parc`] inner data type"],["VWeak","`VWeak` is a version of [`Parc`] that holds a non-owning thread-safe  reference to the managed allocation in the volatile heap. The allocation is accessed by calling `upgrade` on the `VWeak` pointer, which returns an [`Option`]`<`[`Parc`]`<T>>`."],["Weak","`Weak` is a version of [`Parc`] that holds a non-owning reference to the managed allocation. The allocation is accessed by calling `upgrade` on the `Weak` pointer, which returns an [`Option`]`<`[`Parc`]`<T>>`."]]});