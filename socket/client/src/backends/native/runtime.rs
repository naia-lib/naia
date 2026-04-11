use std::{future, sync::LazyLock, thread};

use tokio::runtime::{Builder, Handle};

/// Returns a handle to a background tokio runtime.
/// Required because webrtc-unreliable-client internally uses tokio::spawn.
pub fn get_runtime() -> Handle {
    static GLOBAL: LazyLock<Handle> = LazyLock::new(|| {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("was not able to build the runtime");

        let runtime_handle = runtime.handle().clone();

        thread::Builder::new()
            .name("webrtc-runtime".to_string())
            .spawn(move || {
                let _guard = runtime.enter();
                runtime.block_on(future::pending::<()>());
            })
            .expect("cannot spawn executor thread");

        let _guard = runtime_handle.enter();

        runtime_handle
    });

    GLOBAL.clone()
}
