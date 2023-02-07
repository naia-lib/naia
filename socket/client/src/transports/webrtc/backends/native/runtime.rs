use std::{future, thread};

use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Handle};

/// TODO: make description
pub fn get_runtime() -> Handle {
    static GLOBAL: Lazy<Handle> = Lazy::new(|| {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("was not able to build the runtime");

        let runtime_handle = runtime.handle().clone();

        thread::Builder::new()
            .name("tokio-main".to_string())
            .spawn(move || {
                let _guard = runtime.enter();
                runtime.block_on(future::pending::<()>());
            })
            .expect("cannot spawn executor thread");

        let _guard = runtime_handle.enter();

        runtime_handle
    });

    Lazy::<Handle>::force(&GLOBAL).clone()
}
